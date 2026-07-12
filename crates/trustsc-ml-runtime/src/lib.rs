#![forbid(unsafe_code)]

//! Zero-allocation, device-side inference engine (ADR-017). `Classifier1D` consumes an
//! immutable `trustsc_ml_schema::ModelPackage` and evaluates it with plain, strictly-ordered
//! scalar loops — no SIMD, no `f32::mul_add`/FMA, no heap allocation in [`Classifier1D::predict`].
//! This is the entire "SOUP-free" inference stack: no ONNX Runtime, no PyTorch, just the
//! arithmetic a Dense/Conv1D/pooling/activation network needs, written and reviewed as
//! ordinary Rust under design control.

use trustsc_core::{TrustScResult, Validates, ValidationError};
use trustsc_ml_schema::{Layer, ModelPackage};

#[derive(Clone, Copy, Debug, PartialEq)]
pub struct Prediction<const MAX_OUT: usize> {
    pub class: u8,
    pub scores: [f32; MAX_OUT],
    pub score_len: usize,
}

impl<const MAX_OUT: usize> Prediction<MAX_OUT> {
    pub fn scores(&self) -> &[f32] {
        &self.scores[..self.score_len]
    }
}

/// Activation tensor shape flowing between layers during `predict`. Mirrors the private
/// shape trace `trustsc-ml-schema` uses at compile time, recomputed here so the runtime never
/// depends on schema internals.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
enum Shape {
    Sequence { channels: usize, length: usize },
    Flat { features: usize },
}

impl Shape {
    fn element_count(self) -> usize {
        match self {
            Shape::Sequence { channels, length } => channels * length,
            Shape::Flat { features } => features,
        }
    }
}

/// Borrows an immutable, validated [`ModelPackage`] and runs bounded, allocation-free
/// inference. `MAX_UNITS` bounds the widest intermediate activation (see
/// [`ModelPackage::max_layer_units`]); `MAX_OUT` bounds the output class count.
#[derive(Debug)]
pub struct Classifier1D<'a, const MAX_UNITS: usize, const MAX_OUT: usize> {
    package: &'a ModelPackage,
}

impl<'a, const MAX_UNITS: usize, const MAX_OUT: usize> Classifier1D<'a, MAX_UNITS, MAX_OUT> {
    /// Validates the package, checks every intermediate activation and the output class count
    /// fit within `MAX_UNITS`/`MAX_OUT`, then re-runs every baked golden vector and fails
    /// closed (returns an error) if the runtime's bit-exact output diverges from the
    /// host-recorded expectation (ADR-017 §4). This is the ML analogue of
    /// `TextRuntime::new()` validating a text package once at startup.
    pub fn new(package: &'a ModelPackage) -> TrustScResult<Self> {
        package.validate()?;

        let required_units = package.max_layer_units()?;
        if required_units > MAX_UNITS {
            return Err(ValidationError::new(format!(
                "model {} requires an activation buffer of {required_units} units but MAX_UNITS is {MAX_UNITS}",
                package.model_id
            )));
        }
        let classes = usize::from(package.output_spec.classes);
        if classes > MAX_OUT {
            return Err(ValidationError::new(format!(
                "model {} has {classes} output classes but MAX_OUT is {MAX_OUT}",
                package.model_id
            )));
        }

        let runtime = Self { package };
        runtime.run_golden_self_test()?;
        Ok(runtime)
    }

    /// Frame-loop constructor (ADR-013): skips validation and the golden self-test for an
    /// already-validated package (typically the same package a prior `new()` call proved
    /// sound at startup). The caller must not mutate the package in between.
    pub fn from_validated_package(package: &'a ModelPackage) -> Self {
        Self { package }
    }

    fn run_golden_self_test(&self) -> TrustScResult<()> {
        for (index, vector) in self.package.golden_vectors.iter().enumerate() {
            let prediction = self.predict(&vector.input)?;
            for (class, expected) in vector.expected.iter().enumerate() {
                if prediction.scores[class].to_bits() != expected.to_bits() {
                    return Err(ValidationError::new(format!(
                        "golden vector {index} failed self-test: runtime output for class {class} \
                         diverges from the baked expectation (ADR-017 \u{a7}4 fail-closed determinism check)"
                    )));
                }
            }
        }
        Ok(())
    }

    /// Runs one forward pass over `input` (length must equal `input_spec.sample_count()`).
    /// Uses two fixed-size ping-pong scratch buffers on the stack — no heap allocation.
    pub fn predict(&self, input: &[f32]) -> TrustScResult<Prediction<MAX_OUT>> {
        let expected_len = self.package.input_spec.sample_count();
        if input.len() != expected_len {
            return Err(ValidationError::new(format!(
                "predict input length {} does not match input_spec sample count {expected_len}",
                input.len()
            )));
        }
        // `new()` checks this against MAX_UNITS before ever calling predict, but
        // `from_validated_package()` (ADR-013's per-frame path) skips that check, so a caller
        // reusing a package validated for a different, larger Classifier1D instantiation must
        // still fail closed here rather than panic on the buffer copy below.
        if expected_len > MAX_UNITS {
            return Err(ValidationError::new(format!(
                "predict input length {expected_len} exceeds MAX_UNITS {MAX_UNITS}"
            )));
        }
        let classes = usize::from(self.package.output_spec.classes);
        if classes > MAX_OUT {
            return Err(ValidationError::new(format!(
                "model has {classes} output classes but MAX_OUT is {MAX_OUT}"
            )));
        }

        let mut buf_a = [0f32; MAX_UNITS];
        let mut buf_b = [0f32; MAX_UNITS];
        buf_a[..input.len()].copy_from_slice(input);

        let mut shape = Shape::Sequence {
            channels: usize::from(self.package.input_spec.channels),
            length: usize::from(self.package.input_spec.length),
        };
        let mut use_a = true;

        for layer in &self.package.layers {
            let len = shape.element_count();
            let new_shape = if use_a {
                let (src, dst) = (&buf_a, &mut buf_b);
                self.apply_layer(layer, &src[..len], dst, shape)?
            } else {
                let (src, dst) = (&buf_b, &mut buf_a);
                self.apply_layer(layer, &src[..len], dst, shape)?
            };
            shape = new_shape;
            use_a = !use_a;
        }

        let final_buf = if use_a { &buf_a } else { &buf_b };
        if classes == 0 {
            return Err(ValidationError::new("model has zero output classes"));
        }
        let mut scores = [0f32; MAX_OUT];
        scores[..classes].copy_from_slice(&final_buf[..classes]);

        let mut best_index = 0usize;
        let mut best_value = scores[0];
        for (index, &value) in scores.iter().enumerate().take(classes).skip(1) {
            if value > best_value {
                best_value = value;
                best_index = index;
            }
        }
        let class = u8::try_from(best_index).map_err(|_| {
            ValidationError::new(format!(
                "predicted class index {best_index} does not fit in u8 (models must declare at most 256 output classes)"
            ))
        })?;

        Ok(Prediction {
            class,
            scores,
            score_len: classes,
        })
    }

    fn apply_layer(
        &self,
        layer: &Layer,
        src: &[f32],
        dst: &mut [f32; MAX_UNITS],
        shape: Shape,
    ) -> TrustScResult<Shape> {
        match layer {
            Layer::Conv1D {
                weights_id,
                bias_id,
                in_channels,
                out_channels,
                kernel,
                stride,
                padding,
            } => {
                let Shape::Sequence { channels, length } = shape else {
                    return Err(ValidationError::new(
                        "Conv1D layer requires a sequence activation",
                    ));
                };
                debug_assert_eq!(channels, usize::from(*in_channels));

                let (in_channels, out_channels, kernel, stride, padding) = (
                    usize::from(*in_channels),
                    usize::from(*out_channels),
                    usize::from(*kernel),
                    usize::from(*stride),
                    usize::from(*padding),
                );
                let out_length = conv_output_length(length, kernel, stride, padding)
                    .ok_or_else(|| ValidationError::new("Conv1D output length is undefined"))?;
                let out_units = out_channels * out_length;
                self.check_capacity(out_units)?;

                let weights = self.find_tensor(weights_id)?;
                let bias = self.find_tensor(bias_id)?;

                for oc in 0..out_channels {
                    for t in 0..out_length {
                        let mut sum = bias.data[oc];
                        for ic in 0..in_channels {
                            for k in 0..kernel {
                                let pos = t * stride + k;
                                let value = if pos >= padding && pos - padding < length {
                                    src[ic * length + (pos - padding)]
                                } else {
                                    0.0
                                };
                                let weight_index = oc * (in_channels * kernel) + ic * kernel + k;
                                sum += weights.data[weight_index] * value;
                            }
                        }
                        dst[oc * out_length + t] = sum;
                    }
                }

                Ok(Shape::Sequence {
                    channels: out_channels,
                    length: out_length,
                })
            }
            Layer::MaxPool1D { window, stride } => {
                self.apply_pool(src, dst, shape, *window, *stride, PoolKind::Max)
            }
            Layer::AvgPool1D { window, stride } => {
                self.apply_pool(src, dst, shape, *window, *stride, PoolKind::Avg)
            }
            Layer::Flatten => {
                let Shape::Sequence { channels, length } = shape else {
                    return Err(ValidationError::new("Flatten requires a sequence activation"));
                };
                let features = channels * length;
                self.check_capacity(features)?;
                dst[..features].copy_from_slice(&src[..features]);
                Ok(Shape::Flat { features })
            }
            Layer::Dense {
                weights_id,
                bias_id,
                in_features,
                out_features,
            } => {
                let Shape::Flat { features } = shape else {
                    return Err(ValidationError::new("Dense layer requires a flat activation"));
                };
                debug_assert_eq!(features, *in_features as usize);

                let (in_features, out_features) =
                    (*in_features as usize, *out_features as usize);
                self.check_capacity(out_features)?;

                let weights = self.find_tensor(weights_id)?;
                let bias = self.find_tensor(bias_id)?;

                for o in 0..out_features {
                    let mut sum = bias.data[o];
                    for i in 0..in_features {
                        sum += weights.data[o * in_features + i] * src[i];
                    }
                    dst[o] = sum;
                }

                Ok(Shape::Flat {
                    features: out_features,
                })
            }
            Layer::Relu => {
                let n = shape.element_count();
                for i in 0..n {
                    dst[i] = src[i].max(0.0);
                }
                Ok(shape)
            }
            Layer::Sigmoid => {
                // Uses `f32::exp`, which resolves to the target's libm — not guaranteed
                // bit-identical across host and device toolchains. The golden self-test in
                // `Classifier1D::new()` is the designed defense against that divergence
                // (ADR-017 §4): it fails closed if a target's libm disagrees with the
                // baked-in host expectation, rather than silently drifting.
                let n = shape.element_count();
                for i in 0..n {
                    dst[i] = 1.0 / (1.0 + (-src[i]).exp());
                }
                Ok(shape)
            }
            Layer::Softmax => {
                let n = shape.element_count();
                let mut max_value = src[0];
                for &value in &src[1..n] {
                    if value > max_value {
                        max_value = value;
                    }
                }
                let mut sum = 0.0f32;
                for i in 0..n {
                    let exp_value = (src[i] - max_value).exp();
                    dst[i] = exp_value;
                    sum += exp_value;
                }
                for value in &mut dst[..n] {
                    *value /= sum;
                }
                Ok(shape)
            }
        }
    }

    fn apply_pool(
        &self,
        src: &[f32],
        dst: &mut [f32; MAX_UNITS],
        shape: Shape,
        window: u16,
        stride: u16,
        kind: PoolKind,
    ) -> TrustScResult<Shape> {
        let Shape::Sequence { channels, length } = shape else {
            return Err(ValidationError::new("pooling layer requires a sequence activation"));
        };
        let (window, stride) = (usize::from(window), usize::from(stride));
        let out_length = pool_output_length(length, window, stride)
            .ok_or_else(|| ValidationError::new("pooling output length is undefined"))?;
        let out_units = channels * out_length;
        self.check_capacity(out_units)?;

        for c in 0..channels {
            for t in 0..out_length {
                let start = c * length + t * stride;
                let window_slice = &src[start..start + window];
                dst[c * out_length + t] = match kind {
                    PoolKind::Max => {
                        let mut max_value = window_slice[0];
                        for &value in &window_slice[1..] {
                            if value > max_value {
                                max_value = value;
                            }
                        }
                        max_value
                    }
                    PoolKind::Avg => {
                        let mut sum = 0.0f32;
                        for &value in window_slice {
                            sum += value;
                        }
                        sum / window as f32
                    }
                };
            }
        }

        Ok(Shape::Sequence {
            channels,
            length: out_length,
        })
    }

    fn find_tensor(&self, id: &str) -> TrustScResult<&'a trustsc_ml_schema::Tensor> {
        self.package
            .find_tensor(id)
            .ok_or_else(|| ValidationError::new(format!("unknown tensor {id}")))
    }

    fn check_capacity(&self, units: usize) -> TrustScResult<()> {
        if units > MAX_UNITS {
            return Err(ValidationError::new(format!(
                "activation buffer capacity exceeded ({units} units, MAX_UNITS is {MAX_UNITS})"
            )));
        }
        Ok(())
    }
}

#[derive(Clone, Copy)]
enum PoolKind {
    Max,
    Avg,
}

fn conv_output_length(length: usize, kernel: usize, stride: usize, padding: usize) -> Option<usize> {
    if stride == 0 || kernel == 0 {
        return None;
    }
    let padded = length.checked_add(padding.checked_mul(2)?)?;
    if padded < kernel {
        return None;
    }
    Some((padded - kernel) / stride + 1)
}

fn pool_output_length(length: usize, window: usize, stride: usize) -> Option<usize> {
    if stride == 0 || window == 0 || length < window {
        return None;
    }
    Some((length - window) / stride + 1)
}

#[cfg(test)]
mod tests {
    use super::*;
    use trustsc_ml_schema::{DeterminismEvidence, Dtype, GoldenVector, InputSpec, OutputSpec, Tensor};

    fn sha(seed: u8) -> String {
        (0..64)
            .map(|i| char::from_digit(u32::from((seed + i as u8) % 16), 16).unwrap())
            .collect()
    }

    fn evidence() -> DeterminismEvidence {
        DeterminismEvidence {
            package_sha256: sha(0),
            toolchain_id: "rust-1.87.0".to_string(),
            build_recipe_sha256: sha(1),
            weights_source_sha256: sha(2),
        }
    }

    /// Dense(4->2) + Softmax classifier with weights chosen so class 1 wins whenever the sum
    /// of the four inputs is positive: hand-computable expected values for the golden test.
    fn mlp_package() -> ModelPackage {
        let mut package = ModelPackage {
            model_id: "TEST-MLP".to_string(),
            dtype: Dtype::F32,
            input_spec: InputSpec {
                length: 4,
                channels: 1,
            },
            output_spec: OutputSpec {
                classes: 2,
                labels: vec!["NORMAL".to_string(), "ARRHYTHMIA".to_string()],
            },
            layers: vec![
                Layer::Flatten,
                Layer::Dense {
                    weights_id: "dense.weight".to_string(),
                    bias_id: "dense.bias".to_string(),
                    in_features: 4,
                    out_features: 2,
                },
                Layer::Softmax,
            ],
            tensors: vec![
                Tensor {
                    id: "dense.weight".to_string(),
                    shape: vec![2, 4],
                    // class 0 (NORMAL) row is all-zero, class 1 (ARRHYTHMIA) row sums inputs.
                    data: vec![0.0, 0.0, 0.0, 0.0, 1.0, 1.0, 1.0, 1.0],
                },
                Tensor {
                    id: "dense.bias".to_string(),
                    shape: vec![2],
                    data: vec![0.0, 0.0],
                },
            ],
            golden_vectors: vec![],
            evidence: evidence(),
        };

        // Golden vector: input all zero -> logits [0, 0] -> softmax [0.5, 0.5].
        package.golden_vectors.push(GoldenVector {
            input: vec![0.0, 0.0, 0.0, 0.0],
            expected: vec![0.5, 0.5],
        });

        package
    }

    #[test]
    fn constructs_and_predicts_with_bit_exact_golden_self_test() {
        let package = mlp_package();
        let runtime = Classifier1D::<8, 4>::new(&package).expect("golden self-test should pass");

        let prediction = runtime
            .predict(&[1.0, 1.0, 1.0, 1.0])
            .expect("predict should succeed");
        assert_eq!(prediction.class, 1); // ARRHYTHMIA: logits [0, 4] -> softmax favors class 1
        assert!(prediction.scores()[1] > prediction.scores()[0]);
    }

    #[test]
    fn from_validated_package_skips_self_test_but_still_predicts() {
        let package = mlp_package();
        let runtime = Classifier1D::<8, 4>::from_validated_package(&package);
        let prediction = runtime.predict(&[0.0, 0.0, 0.0, 0.0]).unwrap();
        assert_eq!(prediction.scores()[0], 0.5);
        assert_eq!(prediction.scores()[1], 0.5);
    }

    #[test]
    fn from_validated_package_fails_closed_instead_of_panicking_when_input_exceeds_max_units() {
        // mlp_package's sample_count is 4; from_validated_package skips new()'s MAX_UNITS check,
        // so predict() itself must reject this rather than panic on the buffer copy.
        let package = mlp_package();
        let runtime = Classifier1D::<2, 4>::from_validated_package(&package);
        let error = runtime
            .predict(&[0.0, 0.0, 0.0, 0.0])
            .expect_err("input exceeding MAX_UNITS must fail closed, not panic");
        assert!(error.to_string().contains("MAX_UNITS"));
    }

    #[test]
    fn from_validated_package_fails_closed_instead_of_panicking_when_classes_exceed_max_out() {
        // mlp_package declares 2 output classes; from_validated_package skips new()'s MAX_OUT
        // check, so predict() itself must reject this rather than panic on the scores copy.
        let package = mlp_package();
        let runtime = Classifier1D::<8, 1>::from_validated_package(&package);
        let error = runtime
            .predict(&[0.0, 0.0, 0.0, 0.0])
            .expect_err("classes exceeding MAX_OUT must fail closed, not panic");
        assert!(error.to_string().contains("MAX_OUT"));
    }

    #[test]
    fn rejects_wrong_input_length() {
        let package = mlp_package();
        let runtime = Classifier1D::<8, 4>::new(&package).unwrap();
        let error = runtime.predict(&[0.0, 0.0]).expect_err("wrong length");
        assert!(error.to_string().contains("does not match"));
    }

    #[test]
    fn rejects_max_units_too_small() {
        let package = mlp_package();
        let error = Classifier1D::<2, 4>::new(&package).expect_err("buffer too small");
        assert!(error.to_string().contains("MAX_UNITS"));
    }

    #[test]
    fn rejects_max_out_too_small() {
        let package = mlp_package();
        let error = Classifier1D::<8, 1>::new(&package).expect_err("MAX_OUT too small");
        assert!(error.to_string().contains("MAX_OUT"));
    }

    #[test]
    fn fails_closed_when_golden_vector_diverges() {
        let mut package = mlp_package();
        package.golden_vectors[0].expected = vec![0.9, 0.1]; // wrong on purpose
        let error = Classifier1D::<8, 4>::new(&package).expect_err("divergent golden vector");
        assert!(error.to_string().contains("fail-closed"));
    }

    #[test]
    fn repeated_predict_calls_are_bit_exact() {
        let package = mlp_package();
        let runtime = Classifier1D::<8, 4>::new(&package).unwrap();
        let a = runtime.predict(&[0.3, -0.1, 0.7, 0.2]).unwrap();
        let b = runtime.predict(&[0.3, -0.1, 0.7, 0.2]).unwrap();
        assert_eq!(a.scores[0].to_bits(), b.scores[0].to_bits());
        assert_eq!(a.scores[1].to_bits(), b.scores[1].to_bits());
    }

    #[test]
    fn conv1d_and_pooling_pipeline_matches_hand_computed_output() {
        // input length=8, channels=1; Conv1D(1->1,k=3,stride=1,padding=0) with kernel [1,1,1]
        // and bias 0 => a 3-point moving sum, output length 6.
        // MaxPool1D(window=2,stride=2) => output length 3.
        // Flatten -> Dense(3->2) identity-ish -> Softmax.
        let conv_weight = vec![1.0f32, 1.0, 1.0]; // [out=1][in=1][kernel=3]
        let input = vec![1.0f32, 2.0, 3.0, 4.0, 5.0, 6.0, 7.0, 8.0];
        // moving sums (window 3): 6, 9, 12, 15, 18, 21
        // maxpool window=2 stride=2 over [6,9,12,15,18,21]: max(6,9)=9, max(12,15)=15, max(18,21)=21
        let expected_pool = [9.0f32, 15.0, 21.0];

        let mut package = ModelPackage {
            model_id: "TEST-CNN".to_string(),
            dtype: Dtype::F32,
            input_spec: InputSpec {
                length: 8,
                channels: 1,
            },
            output_spec: OutputSpec {
                classes: 2,
                labels: vec!["A".to_string(), "B".to_string()],
            },
            layers: vec![
                Layer::Conv1D {
                    weights_id: "conv.weight".to_string(),
                    bias_id: "conv.bias".to_string(),
                    in_channels: 1,
                    out_channels: 1,
                    kernel: 3,
                    stride: 1,
                    padding: 0,
                },
                Layer::MaxPool1D {
                    window: 2,
                    stride: 2,
                },
                Layer::Flatten,
                Layer::Dense {
                    weights_id: "dense.weight".to_string(),
                    bias_id: "dense.bias".to_string(),
                    in_features: 3,
                    out_features: 2,
                },
            ],
            tensors: vec![
                Tensor {
                    id: "conv.weight".to_string(),
                    shape: vec![1, 1, 3],
                    data: conv_weight,
                },
                Tensor {
                    id: "conv.bias".to_string(),
                    shape: vec![1],
                    data: vec![0.0],
                },
                Tensor {
                    id: "dense.weight".to_string(),
                    shape: vec![2, 3],
                    data: vec![1.0, 0.0, 0.0, 0.0, 0.0, 1.0],
                },
                Tensor {
                    id: "dense.bias".to_string(),
                    shape: vec![2],
                    data: vec![0.0, 0.0],
                },
            ],
            golden_vectors: vec![],
            evidence: evidence(),
        };
        package.golden_vectors.push(GoldenVector {
            input: input.clone(),
            expected: vec![expected_pool[0], expected_pool[2]],
        });

        let runtime = Classifier1D::<8, 4>::new(&package).expect("self-test should pass");
        let prediction = runtime.predict(&input).unwrap();
        assert_eq!(prediction.scores()[0], expected_pool[0]);
        assert_eq!(prediction.scores()[1], expected_pool[2]);
    }
}
