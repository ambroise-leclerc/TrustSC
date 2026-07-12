#![forbid(unsafe_code)]

//! Shared contract for the ML inference pipeline (ADR-017): the compiled, immutable
//! `ModelPackage` both `trustsc-ml-authoring` (host-side compiler) and `trustsc-ml-runtime`
//! (device-side inference engine) agree on. This crate contains no I/O and no arithmetic —
//! only data types and referential-integrity validation, mirroring `trustsc-text-schema`.

use std::collections::BTreeSet;

use trustsc_core::{TrustScResult, Validates, ValidationError, validate_non_empty};

/// v1 supports `f32` tensors only (ADR-017 §5); the variant exists so a future quantized
/// dtype can be added without breaking the schema shape.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum Dtype {
    F32,
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct InputSpec {
    pub length: u16,
    pub channels: u16,
}

impl InputSpec {
    pub fn sample_count(&self) -> usize {
        usize::from(self.length) * usize::from(self.channels)
    }
}

impl Validates for InputSpec {
    fn validate(&self) -> TrustScResult<()> {
        if self.length == 0 {
            return Err(ValidationError::new("input_spec length must be positive"));
        }
        if self.channels == 0 {
            return Err(ValidationError::new(
                "input_spec channels must be positive",
            ));
        }
        Ok(())
    }
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct OutputSpec {
    pub classes: u16,
    pub labels: Vec<String>,
}

impl Validates for OutputSpec {
    fn validate(&self) -> TrustScResult<()> {
        if self.classes == 0 {
            return Err(ValidationError::new(
                "output_spec classes must be positive",
            ));
        }
        if self.labels.len() != usize::from(self.classes) {
            return Err(ValidationError::new(
                "output_spec labels count must equal classes",
            ));
        }
        for label in &self.labels {
            validate_non_empty("output_spec label", label)?;
        }
        ensure_unique_ids(self.labels.iter().map(String::as_str), "output_spec label")?;
        Ok(())
    }
}

#[derive(Clone, Debug, PartialEq)]
pub struct Tensor {
    pub id: String,
    pub shape: Vec<u32>,
    pub data: Vec<f32>,
}

impl Validates for Tensor {
    fn validate(&self) -> TrustScResult<()> {
        validate_non_empty("tensor id", &self.id)?;
        if self.shape.is_empty() {
            return Err(ValidationError::new(format!(
                "tensor {} shape must not be empty",
                self.id
            )));
        }
        if self.shape.iter().any(|&dim| dim == 0) {
            return Err(ValidationError::new(format!(
                "tensor {} shape dimensions must be positive",
                self.id
            )));
        }

        let expected: usize = self
            .shape
            .iter()
            .try_fold(1usize, |acc, &dim| acc.checked_mul(dim as usize))
            .ok_or_else(|| {
                ValidationError::new(format!("tensor {} shape overflows usize", self.id))
            })?;
        if expected != self.data.len() {
            return Err(ValidationError::new(format!(
                "tensor {} data length does not match the product of its shape",
                self.id
            )));
        }

        Ok(())
    }
}

impl Tensor {
    fn shape_matches(&self, expected: &[u32]) -> bool {
        self.shape == expected
    }
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub enum Layer {
    Conv1D {
        weights_id: String,
        bias_id: String,
        in_channels: u16,
        out_channels: u16,
        kernel: u16,
        stride: u16,
        padding: u16,
    },
    MaxPool1D {
        window: u16,
        stride: u16,
    },
    AvgPool1D {
        window: u16,
        stride: u16,
    },
    Flatten,
    Dense {
        weights_id: String,
        bias_id: String,
        in_features: u32,
        out_features: u32,
    },
    Relu,
    Sigmoid,
    Softmax,
}

#[derive(Clone, Debug, PartialEq)]
pub struct GoldenVector {
    pub input: Vec<f32>,
    pub expected: Vec<f32>,
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct DeterminismEvidence {
    pub package_sha256: String,
    pub toolchain_id: String,
    pub build_recipe_sha256: String,
    pub weights_source_sha256: String,
}

impl Validates for DeterminismEvidence {
    fn validate(&self) -> TrustScResult<()> {
        validate_non_empty("package_sha256", &self.package_sha256)?;
        validate_non_empty("toolchain_id", &self.toolchain_id)?;
        validate_non_empty("build_recipe_sha256", &self.build_recipe_sha256)?;
        validate_non_empty("weights_source_sha256", &self.weights_source_sha256)?;

        if !is_sha256(&self.package_sha256)
            || !is_sha256(&self.build_recipe_sha256)
            || !is_sha256(&self.weights_source_sha256)
        {
            return Err(ValidationError::new(
                "determinism evidence digests must be 64-character lowercase hexadecimal values",
            ));
        }

        Ok(())
    }
}

#[derive(Clone, Debug, PartialEq)]
pub struct ModelPackage {
    pub model_id: String,
    pub dtype: Dtype,
    pub input_spec: InputSpec,
    pub output_spec: OutputSpec,
    pub layers: Vec<Layer>,
    pub tensors: Vec<Tensor>,
    pub golden_vectors: Vec<GoldenVector>,
    pub evidence: DeterminismEvidence,
}

/// The shape of the activation tensor flowing between layers. `Sequence` is the shape used by
/// `Conv1D`/pooling layers; `Flatten` converts it to `Flat`, the shape `Dense` layers and the
/// final `output_spec` require.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
enum ActivationShape {
    Sequence { channels: u32, length: u32 },
    Flat { features: u32 },
}

impl ActivationShape {
    fn element_count(&self) -> u64 {
        match *self {
            ActivationShape::Sequence { channels, length } => {
                u64::from(channels) * u64::from(length)
            }
            ActivationShape::Flat { features } => u64::from(features),
        }
    }
}

impl ModelPackage {
    pub fn find_tensor(&self, id: &str) -> Option<&Tensor> {
        self.tensors.iter().find(|tensor| tensor.id == id)
    }

    /// The widest activation tensor (by element count) that flows between any two layers —
    /// the value a `trustsc-ml-runtime` const-generic buffer capacity must be sized to hold.
    /// Returns an error under the same conditions `validate()` does, since the shape trace
    /// requires a structurally valid layer chain.
    pub fn max_layer_units(&self) -> TrustScResult<usize> {
        let shapes = self.trace_activation_shapes()?;
        let widest = shapes
            .iter()
            .map(ActivationShape::element_count)
            .max()
            .unwrap_or(0);
        usize::try_from(widest)
            .map_err(|_| ValidationError::new("widest activation shape overflows usize"))
    }

    fn trace_activation_shapes(&self) -> TrustScResult<Vec<ActivationShape>> {
        let mut shape = ActivationShape::Sequence {
            channels: u32::from(self.input_spec.channels),
            length: u32::from(self.input_spec.length),
        };
        let mut shapes = vec![shape];

        for layer in &self.layers {
            shape = self.step_shape(shape, layer)?;
            shapes.push(shape);
        }

        Ok(shapes)
    }

    fn step_shape(&self, shape: ActivationShape, layer: &Layer) -> TrustScResult<ActivationShape> {
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
                let ActivationShape::Sequence { channels, length } = shape else {
                    return Err(ValidationError::new(
                        "Conv1D layer requires a sequence activation (channels x length)",
                    ));
                };
                if channels != u32::from(*in_channels) {
                    return Err(ValidationError::new(
                        "Conv1D in_channels does not match the incoming activation channels",
                    ));
                }

                let weights = self.find_tensor(weights_id).ok_or_else(|| {
                    ValidationError::new(format!(
                        "Conv1D references unknown weights tensor {weights_id}"
                    ))
                })?;
                let bias = self.find_tensor(bias_id).ok_or_else(|| {
                    ValidationError::new(format!("Conv1D references unknown bias tensor {bias_id}"))
                })?;
                if !weights.shape_matches(&[
                    u32::from(*out_channels),
                    u32::from(*in_channels),
                    u32::from(*kernel),
                ]) {
                    return Err(ValidationError::new(format!(
                        "Conv1D weights tensor {weights_id} shape does not match [out_channels, in_channels, kernel]"
                    )));
                }
                if !bias.shape_matches(&[u32::from(*out_channels)]) {
                    return Err(ValidationError::new(format!(
                        "Conv1D bias tensor {bias_id} shape does not match [out_channels]"
                    )));
                }

                let out_length = conv_output_length(
                    length,
                    u32::from(*kernel),
                    u32::from(*stride),
                    u32::from(*padding),
                )
                .ok_or_else(|| {
                    ValidationError::new("Conv1D output length is undefined for the given kernel/stride/padding")
                })?;

                Ok(ActivationShape::Sequence {
                    channels: u32::from(*out_channels),
                    length: out_length,
                })
            }
            Layer::MaxPool1D { window, stride } | Layer::AvgPool1D { window, stride } => {
                let ActivationShape::Sequence { channels, length } = shape else {
                    return Err(ValidationError::new(
                        "pooling layer requires a sequence activation (channels x length)",
                    ));
                };
                let out_length = pool_output_length(length, u32::from(*window), u32::from(*stride))
                    .ok_or_else(|| {
                        ValidationError::new(
                            "pooling output length is undefined for the given window/stride",
                        )
                    })?;
                Ok(ActivationShape::Sequence {
                    channels,
                    length: out_length,
                })
            }
            Layer::Flatten => {
                let ActivationShape::Sequence { channels, length } = shape else {
                    return Err(ValidationError::new(
                        "Flatten requires a sequence activation (channels x length)",
                    ));
                };
                let features = channels.checked_mul(length).ok_or_else(|| {
                    ValidationError::new("Flatten feature count overflows u32")
                })?;
                Ok(ActivationShape::Flat { features })
            }
            Layer::Dense {
                weights_id,
                bias_id,
                in_features,
                out_features,
            } => {
                let ActivationShape::Flat { features } = shape else {
                    return Err(ValidationError::new(
                        "Dense layer requires a flat activation (call Flatten first)",
                    ));
                };
                if features != *in_features {
                    return Err(ValidationError::new(
                        "Dense in_features does not match the incoming activation feature count",
                    ));
                }

                let weights = self.find_tensor(weights_id).ok_or_else(|| {
                    ValidationError::new(format!(
                        "Dense references unknown weights tensor {weights_id}"
                    ))
                })?;
                let bias = self.find_tensor(bias_id).ok_or_else(|| {
                    ValidationError::new(format!("Dense references unknown bias tensor {bias_id}"))
                })?;
                if !weights.shape_matches(&[*out_features, *in_features]) {
                    return Err(ValidationError::new(format!(
                        "Dense weights tensor {weights_id} shape does not match [out_features, in_features]"
                    )));
                }
                if !bias.shape_matches(&[*out_features]) {
                    return Err(ValidationError::new(format!(
                        "Dense bias tensor {bias_id} shape does not match [out_features]"
                    )));
                }

                Ok(ActivationShape::Flat {
                    features: *out_features,
                })
            }
            Layer::Relu | Layer::Sigmoid | Layer::Softmax => Ok(shape),
        }
    }
}

impl Validates for ModelPackage {
    fn validate(&self) -> TrustScResult<()> {
        validate_non_empty("model_id", &self.model_id)?;
        if !matches!(self.dtype, Dtype::F32) {
            return Err(ValidationError::new(
                "model package dtype must be F32 in v1 (ADR-017 §5)",
            ));
        }
        self.input_spec.validate()?;
        self.output_spec.validate()?;

        if self.layers.is_empty() {
            return Err(ValidationError::new(
                "model package must contain at least one layer",
            ));
        }
        if self.tensors.is_empty() {
            return Err(ValidationError::new(
                "model package must contain at least one tensor",
            ));
        }
        for tensor in &self.tensors {
            tensor.validate()?;
        }
        ensure_unique_ids(self.tensors.iter().map(|t| t.id.as_str()), "tensor")?;

        let shapes = self.trace_activation_shapes()?;
        let final_shape = *shapes.last().expect("trace always includes the input shape");
        let ActivationShape::Flat { features } = final_shape else {
            return Err(ValidationError::new(
                "model package's final layer must produce a flat activation (end with Flatten + Dense, or Flatten directly for classes == input size)",
            ));
        };
        if features != u32::from(self.output_spec.classes) {
            return Err(ValidationError::new(
                "model package's final activation feature count does not match output_spec classes",
            ));
        }

        if self.golden_vectors.is_empty() {
            return Err(ValidationError::new(
                "model package must contain at least one golden vector (ADR-017 §4)",
            ));
        }
        for (index, vector) in self.golden_vectors.iter().enumerate() {
            if vector.input.len() != self.input_spec.sample_count() {
                return Err(ValidationError::new(format!(
                    "golden vector {index} input length does not match input_spec"
                )));
            }
            if vector.expected.len() != usize::from(self.output_spec.classes) {
                return Err(ValidationError::new(format!(
                    "golden vector {index} expected length does not match output_spec classes"
                )));
            }
        }

        self.evidence.validate()?;

        Ok(())
    }
}

fn conv_output_length(length: u32, kernel: u32, stride: u32, padding: u32) -> Option<u32> {
    if stride == 0 || kernel == 0 {
        return None;
    }
    let padded = length.checked_add(padding.checked_mul(2)?)?;
    if padded < kernel {
        return None;
    }
    Some((padded - kernel) / stride + 1)
}

fn pool_output_length(length: u32, window: u32, stride: u32) -> Option<u32> {
    if stride == 0 || window == 0 || length < window {
        return None;
    }
    Some((length - window) / stride + 1)
}

fn ensure_unique_ids<'a>(ids: impl IntoIterator<Item = &'a str>, label: &str) -> TrustScResult<()> {
    let mut seen = BTreeSet::new();
    for id in ids {
        if !seen.insert(id.to_string()) {
            return Err(ValidationError::new(format!("{label} ids must be unique")));
        }
    }
    Ok(())
}

fn is_sha256(value: &str) -> bool {
    value.len() == 64
        && value
            .bytes()
            .all(|byte| byte.is_ascii_hexdigit() && !byte.is_ascii_uppercase())
}

#[cfg(test)]
mod tests {
    use super::*;

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

    /// A minimal valid MLP: 4 input samples/channel-1 -> Flatten -> Dense(4->2) -> Softmax.
    fn minimal_package() -> ModelPackage {
        ModelPackage {
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
                    data: vec![0.0; 8],
                },
                Tensor {
                    id: "dense.bias".to_string(),
                    shape: vec![2],
                    data: vec![0.0; 2],
                },
            ],
            golden_vectors: vec![GoldenVector {
                input: vec![0.0; 4],
                expected: vec![0.5, 0.5],
            }],
            evidence: evidence(),
        }
    }

    #[test]
    fn validates_minimal_mlp_package() {
        assert!(minimal_package().validate().is_ok());
    }

    #[test]
    fn rejects_tensor_shape_data_mismatch() {
        let mut package = minimal_package();
        package.tensors[0].data.pop();
        let error = package.validate().expect_err("shape/data mismatch");
        assert!(error.to_string().contains("does not match the product"));
    }

    #[test]
    fn rejects_tensor_shape_product_overflow_instead_of_wrapping() {
        let tensor = Tensor {
            id: "huge".to_string(),
            shape: vec![u32::MAX, u32::MAX, u32::MAX],
            data: vec![],
        };
        let error = tensor.validate().expect_err("shape product overflows usize");
        assert!(error.to_string().contains("overflows usize"));
    }

    #[test]
    fn rejects_dense_referencing_unknown_tensor() {
        let mut package = minimal_package();
        package.layers[1] = Layer::Dense {
            weights_id: "missing".to_string(),
            bias_id: "dense.bias".to_string(),
            in_features: 4,
            out_features: 2,
        };
        let error = package.validate().expect_err("unknown tensor");
        assert!(error.to_string().contains("unknown weights tensor"));
    }

    #[test]
    fn rejects_layer_chain_ending_in_sequence_shape() {
        let mut package = minimal_package();
        package.layers = vec![Layer::Relu];
        let error = package.validate().expect_err("must end flat");
        assert!(error.to_string().contains("flat activation"));
    }

    #[test]
    fn rejects_final_feature_count_mismatch_with_output_classes() {
        let mut package = minimal_package();
        package.output_spec.classes = 3;
        package.output_spec.labels.push("FAULT".to_string());
        let error = package.validate().expect_err("class mismatch");
        assert!(error.to_string().contains("output_spec classes"));
    }

    #[test]
    fn rejects_empty_golden_vectors() {
        let mut package = minimal_package();
        package.golden_vectors.clear();
        let error = package.validate().expect_err("golden vectors required");
        assert!(error.to_string().contains("golden vector"));
    }

    #[test]
    fn computes_conv1d_and_pooling_shape_chain() {
        // input: length=128, channels=1
        // Conv1D(1->4, k=9, stride=1, padding=0) -> length 120
        // MaxPool1D(window=4, stride=4) -> length 30
        // Flatten -> 4*30 = 120 features
        // Dense(120->2)
        let mut package = ModelPackage {
            model_id: "TEST-CNN".to_string(),
            dtype: Dtype::F32,
            input_spec: InputSpec {
                length: 128,
                channels: 1,
            },
            output_spec: OutputSpec {
                classes: 2,
                labels: vec!["NORMAL".to_string(), "ARRHYTHMIA".to_string()],
            },
            layers: vec![
                Layer::Conv1D {
                    weights_id: "conv.weight".to_string(),
                    bias_id: "conv.bias".to_string(),
                    in_channels: 1,
                    out_channels: 4,
                    kernel: 9,
                    stride: 1,
                    padding: 0,
                },
                Layer::Relu,
                Layer::MaxPool1D {
                    window: 4,
                    stride: 4,
                },
                Layer::Flatten,
                Layer::Dense {
                    weights_id: "dense.weight".to_string(),
                    bias_id: "dense.bias".to_string(),
                    in_features: 120,
                    out_features: 2,
                },
                Layer::Softmax,
            ],
            tensors: vec![
                Tensor {
                    id: "conv.weight".to_string(),
                    shape: vec![4, 1, 9],
                    data: vec![0.0; 36],
                },
                Tensor {
                    id: "conv.bias".to_string(),
                    shape: vec![4],
                    data: vec![0.0; 4],
                },
                Tensor {
                    id: "dense.weight".to_string(),
                    shape: vec![2, 120],
                    data: vec![0.0; 240],
                },
                Tensor {
                    id: "dense.bias".to_string(),
                    shape: vec![2],
                    data: vec![0.0; 2],
                },
            ],
            golden_vectors: vec![GoldenVector {
                input: vec![0.0; 128],
                expected: vec![0.5, 0.5],
            }],
            evidence: evidence(),
        };

        assert!(package.validate().is_ok());
        // widest activation is the conv output: 4 channels * 120 length = 480
        assert_eq!(package.max_layer_units().unwrap(), 480);

        // A mismatched in_channels should be rejected.
        package.layers[0] = Layer::Conv1D {
            weights_id: "conv.weight".to_string(),
            bias_id: "conv.bias".to_string(),
            in_channels: 2,
            out_channels: 4,
            kernel: 9,
            stride: 1,
            padding: 0,
        };
        assert!(package.validate().is_err());
    }

    #[test]
    fn rejects_duplicate_tensor_ids() {
        let mut package = minimal_package();
        let dup = package.tensors[0].clone();
        package.tensors.push(dup);
        let error = package.validate().expect_err("duplicate tensor ids");
        assert!(error.to_string().contains("tensor ids must be unique"));
    }

    #[test]
    fn rejects_invalid_sha256_evidence() {
        let mut package = minimal_package();
        package.evidence.package_sha256 = "not-a-hash".to_string();
        assert!(package.validate().is_err());
    }
}
