#![forbid(unsafe_code)]

//! Host-side ML pipeline (ADR-017): imports weights (Hugging Face `safetensors`, hand-parsed
//! to avoid adding a runtime-adjacent SOUP dependency), deterministically compiles an
//! immutable `ModelPackage`, and generates the golden self-test vectors baked into it by
//! running the package through the exact same arithmetic `mdux-ml-runtime` uses on-device —
//! so host and device can never silently diverge.

use std::path::Path;
use std::{fs, path::PathBuf};

use mdux_core::{MduxResult, Validates, ValidationError, validate_non_empty};
use mdux_ml_runtime::Classifier1D;
use mdux_ml_schema::{
    DeterminismEvidence, Dtype, GoldenVector, InputSpec, Layer, ModelPackage, OutputSpec, Tensor,
};
use sha2::{Digest, Sha256};

/// Generous fixed buffer bounds used only for host-side (authoring-time) inference — the
/// device-side `MAX_UNITS`/`MAX_OUT` an application chooses can be much tighter, sized to its
/// actual model via `ModelPackage::max_layer_units`.
const HOST_MAX_UNITS: usize = 1 << 16;
const HOST_MAX_OUT: usize = 64;

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct WeightsFingerprint {
    pub path: PathBuf,
    pub sha256: String,
    pub byte_len: u64,
}

pub fn fingerprint_weights_file(path: impl AsRef<Path>) -> MduxResult<WeightsFingerprint> {
    let path = path.as_ref();
    let contents = fs::read(path).map_err(|error| {
        ValidationError::new(format!(
            "failed to read weights file {}: {error}",
            path.display()
        ))
    })?;

    Ok(WeightsFingerprint {
        path: path.to_path_buf(),
        sha256: sha256_bytes(&contents),
        byte_len: contents.len() as u64,
    })
}

/// Maps a Hugging Face `safetensors` tensor name to the `Tensor::id` this project's
/// `ModelPackage` will use for it, so the same architecture manifest can import weights from
/// differently-named checkpoints (Phase 1 demonstrator vs. Phase 2 production).
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct ArchitectureManifest {
    pub tensor_map: Vec<(String, String)>,
}

#[derive(Clone, Debug, PartialEq)]
pub struct ImportedWeights {
    pub tensors: Vec<Tensor>,
    pub source_sha256: String,
}

/// Hand-parses the `safetensors` container format directly: an 8-byte little-endian header
/// length, a JSON header describing each tensor's dtype/shape/byte-range, then raw tensor
/// bytes. This avoids adding a dedicated `safetensors` crate dependency (ADR-017 §6) — the
/// format needs only the already-registered `serde_json`. Only `F32` tensors are supported in
/// v1; any other dtype is a clear, typed rejection rather than a silent reinterpretation.
pub fn import_safetensors(
    bytes: &[u8],
    manifest: &ArchitectureManifest,
) -> MduxResult<ImportedWeights> {
    if bytes.len() < 8 {
        return Err(ValidationError::new(
            "safetensors file is too short to contain an 8-byte header length",
        ));
    }
    let header_len = u64::from_le_bytes(
        bytes[0..8]
            .try_into()
            .expect("slice of length 8 converts to [u8; 8]"),
    ) as usize;
    let data_start = 8usize
        .checked_add(header_len)
        .ok_or_else(|| ValidationError::new("safetensors header length overflows"))?;
    if bytes.len() < data_start {
        return Err(ValidationError::new(
            "safetensors file is truncated before the end of its declared header",
        ));
    }

    let header: serde_json::Map<String, serde_json::Value> =
        serde_json::from_slice(&bytes[8..data_start]).map_err(|error| {
            ValidationError::new(format!("safetensors header is not valid JSON: {error}"))
        })?;

    let mut tensors = Vec::with_capacity(manifest.tensor_map.len());
    for (safetensors_name, our_id) in &manifest.tensor_map {
        let entry = header.get(safetensors_name).ok_or_else(|| {
            ValidationError::new(format!(
                "safetensors file does not contain tensor {safetensors_name}"
            ))
        })?;
        tensors.push(parse_safetensors_entry(
            safetensors_name,
            our_id,
            entry,
            bytes,
            data_start,
        )?);
    }

    Ok(ImportedWeights {
        tensors,
        source_sha256: sha256_bytes(bytes),
    })
}

fn parse_safetensors_entry(
    safetensors_name: &str,
    our_id: &str,
    entry: &serde_json::Value,
    bytes: &[u8],
    data_start: usize,
) -> MduxResult<Tensor> {
    let object = entry.as_object().ok_or_else(|| {
        ValidationError::new(format!(
            "safetensors entry {safetensors_name} is not a JSON object"
        ))
    })?;

    let dtype = object
        .get("dtype")
        .and_then(serde_json::Value::as_str)
        .ok_or_else(|| {
            ValidationError::new(format!("safetensors entry {safetensors_name} has no dtype"))
        })?;
    if dtype != "F32" {
        return Err(ValidationError::new(format!(
            "safetensors tensor {safetensors_name} has dtype {dtype}; only F32 is supported in v1 (ADR-017 \u{a7}5)"
        )));
    }

    let shape: Vec<u32> = object
        .get("shape")
        .and_then(serde_json::Value::as_array)
        .ok_or_else(|| {
            ValidationError::new(format!(
                "safetensors entry {safetensors_name} has no shape array"
            ))
        })?
        .iter()
        .map(|value| {
            value
                .as_u64()
                .and_then(|n| u32::try_from(n).ok())
                .ok_or_else(|| {
                    ValidationError::new(format!(
                        "safetensors entry {safetensors_name} has a non-u32 shape dimension"
                    ))
                })
        })
        .collect::<MduxResult<_>>()?;

    let offsets = object
        .get("data_offsets")
        .and_then(serde_json::Value::as_array)
        .ok_or_else(|| {
            ValidationError::new(format!(
                "safetensors entry {safetensors_name} has no data_offsets"
            ))
        })?;
    if offsets.len() != 2 {
        return Err(ValidationError::new(format!(
            "safetensors entry {safetensors_name} data_offsets must have exactly two elements"
        )));
    }
    let start = offsets[0].as_u64().ok_or_else(|| {
        ValidationError::new(format!(
            "safetensors entry {safetensors_name} has a non-numeric data_offsets start"
        ))
    })? as usize;
    let end = offsets[1].as_u64().ok_or_else(|| {
        ValidationError::new(format!(
            "safetensors entry {safetensors_name} has a non-numeric data_offsets end"
        ))
    })? as usize;
    if end < start {
        return Err(ValidationError::new(format!(
            "safetensors entry {safetensors_name} has data_offsets end before start"
        )));
    }

    let absolute_start = data_start.checked_add(start).ok_or_else(|| {
        ValidationError::new(format!(
            "safetensors entry {safetensors_name} data_offsets overflow"
        ))
    })?;
    let absolute_end = data_start.checked_add(end).ok_or_else(|| {
        ValidationError::new(format!(
            "safetensors entry {safetensors_name} data_offsets overflow"
        ))
    })?;
    if absolute_end > bytes.len() {
        return Err(ValidationError::new(format!(
            "safetensors entry {safetensors_name} data_offsets exceed the file length"
        )));
    }

    let raw = &bytes[absolute_start..absolute_end];
    if raw.len() % 4 != 0 {
        return Err(ValidationError::new(format!(
            "safetensors entry {safetensors_name} byte length is not a multiple of 4 (F32)"
        )));
    }
    let data: Vec<f32> = raw
        .chunks_exact(4)
        .map(|chunk| {
            f32::from_le_bytes(
                chunk
                    .try_into()
                    .expect("chunks_exact(4) always yields 4 bytes"),
            )
        })
        .collect();

    let expected_len: u64 = shape.iter().map(|&dim| u64::from(dim)).product();
    if expected_len != data.len() as u64 {
        return Err(ValidationError::new(format!(
            "safetensors entry {safetensors_name} data length does not match its shape"
        )));
    }

    Ok(Tensor {
        id: our_id.to_string(),
        shape,
        data,
    })
}

/// Inputs to [`compile_model_package`]. Every field is a plain, reviewable value — no file
/// I/O happens inside compilation itself, matching `mdux-text-authoring::TextCompilationInput`.
#[derive(Clone, Debug, PartialEq)]
pub struct ModelCompilationInput {
    pub model_id: String,
    pub input_spec: InputSpec,
    pub output_spec: OutputSpec,
    pub layers: Vec<Layer>,
    pub tensors: Vec<Tensor>,
    pub toolchain_id: String,
    pub build_recipe: String,
    pub weights_source_sha256: String,
    /// Seed and count for the deterministic golden-vector self-test data baked into the
    /// package (ADR-017 §4). The same seed always produces the same inputs.
    pub golden_seed: u64,
    pub golden_vector_count: usize,
}

/// Deterministically compiles a [`ModelPackage`]: sorts tensors by id, structurally validates
/// the layer chain, generates golden self-test vectors by running the package through
/// `mdux-ml-runtime`'s own kernels (so host and device arithmetic can never drift apart), and
/// computes the package's `DeterminismEvidence` digest over every field. Reordering the input
/// `tensors` before calling this function produces a byte-identical `package_sha256`.
pub fn compile_model_package(input: ModelCompilationInput) -> MduxResult<ModelPackage> {
    let ModelCompilationInput {
        model_id,
        input_spec,
        output_spec,
        layers,
        mut tensors,
        toolchain_id,
        build_recipe,
        weights_source_sha256,
        golden_seed,
        golden_vector_count,
    } = input;

    validate_non_empty("model_id", &model_id)?;
    validate_non_empty("toolchain_id", &toolchain_id)?;
    validate_non_empty("build_recipe", &build_recipe)?;
    validate_non_empty("weights_source_sha256", &weights_source_sha256)?;
    if golden_vector_count == 0 {
        return Err(ValidationError::new(
            "golden_vector_count must be positive",
        ));
    }

    tensors.sort_by(|left, right| left.id.cmp(&right.id));

    // A structurally-valid draft with placeholder golden content, just to satisfy
    // `ModelPackage::validate`'s non-empty/length checks so it can confirm the layer chain,
    // tensor references, and shapes are sound before real golden vectors exist.
    let placeholder = "0".repeat(64);
    let draft = ModelPackage {
        model_id: model_id.clone(),
        dtype: Dtype::F32,
        input_spec: input_spec.clone(),
        output_spec: output_spec.clone(),
        layers: layers.clone(),
        tensors: tensors.clone(),
        golden_vectors: vec![GoldenVector {
            input: vec![0.0; input_spec.sample_count()],
            expected: vec![0.0; usize::from(output_spec.classes)],
        }],
        evidence: DeterminismEvidence {
            package_sha256: placeholder.clone(),
            toolchain_id: toolchain_id.clone(),
            build_recipe_sha256: placeholder.clone(),
            weights_source_sha256: weights_source_sha256.clone(),
        },
    };
    draft.validate()?;

    let golden_vectors = generate_golden_vectors(&draft, golden_seed, golden_vector_count)?;

    let recipe_hash = sha256_text(&build_recipe);
    let package_hash = canonical_package_hash(
        &model_id,
        &input_spec,
        &output_spec,
        &layers,
        &tensors,
        &golden_vectors,
        &toolchain_id,
        &recipe_hash,
        &weights_source_sha256,
    );

    let package = ModelPackage {
        model_id,
        dtype: Dtype::F32,
        input_spec,
        output_spec,
        layers,
        tensors,
        golden_vectors,
        evidence: DeterminismEvidence {
            package_sha256: package_hash,
            toolchain_id,
            build_recipe_sha256: recipe_hash,
            weights_source_sha256,
        },
    };
    package.validate()?;
    Ok(package)
}

/// Runs `count` deterministic pseudo-random inputs (seeded by `seed`) through `package`'s own
/// layers via `mdux-ml-runtime`, recording input/output pairs as golden self-test vectors.
/// `package` need not have real golden vectors yet — only a structurally valid layer/tensor
/// chain — since this is exactly how [`compile_model_package`] produces them from a draft.
pub fn generate_golden_vectors(
    package: &ModelPackage,
    seed: u64,
    count: usize,
) -> MduxResult<Vec<GoldenVector>> {
    let required_units = package.max_layer_units()?;
    if required_units > HOST_MAX_UNITS {
        return Err(ValidationError::new(format!(
            "model {} requires {required_units} activation units, exceeding the authoring host buffer of {HOST_MAX_UNITS}",
            package.model_id
        )));
    }
    let classes = usize::from(package.output_spec.classes);
    if classes > HOST_MAX_OUT {
        return Err(ValidationError::new(format!(
            "model {} has {classes} output classes, exceeding the authoring host buffer of {HOST_MAX_OUT}",
            package.model_id
        )));
    }

    let runtime = Classifier1D::<HOST_MAX_UNITS, HOST_MAX_OUT>::from_validated_package(package);
    let mut rng = DeterministicRng::new(seed);
    let sample_count = package.input_spec.sample_count();

    let mut vectors = Vec::with_capacity(count);
    for _ in 0..count {
        let input: Vec<f32> = (0..sample_count).map(|_| rng.next_symmetric()).collect();
        let prediction = runtime.predict(&input)?;
        vectors.push(GoldenVector {
            input,
            expected: prediction.scores()[..classes].to_vec(),
        });
    }
    Ok(vectors)
}

/// A small, host-only, non-cryptographic PRNG (splitmix64) used purely to generate
/// reproducible golden-vector inputs from a seed — determinism, not randomness quality, is
/// the requirement here.
struct DeterministicRng {
    state: u64,
}

impl DeterministicRng {
    fn new(seed: u64) -> Self {
        Self {
            state: seed ^ 0x9E37_79B9_7F4A_7C15,
        }
    }

    fn next_u64(&mut self) -> u64 {
        self.state = self.state.wrapping_add(0x9E37_79B9_7F4A_7C15);
        let mut z = self.state;
        z = (z ^ (z >> 30)).wrapping_mul(0xBF58_476D_1CE4_E5B9);
        z = (z ^ (z >> 27)).wrapping_mul(0x94D0_49BB_1331_11EB);
        z ^ (z >> 31)
    }

    /// A value in `[-1.0, 1.0)`, a reasonable normalized-signal range for signal-classifier
    /// golden inputs.
    fn next_symmetric(&mut self) -> f32 {
        let bits = (self.next_u64() >> 40) as u32; // top 24 bits
        let unit = bits as f32 / (1u32 << 24) as f32; // [0, 1)
        unit * 2.0 - 1.0
    }
}

fn canonical_package_hash(
    model_id: &str,
    input_spec: &InputSpec,
    output_spec: &OutputSpec,
    layers: &[Layer],
    tensors: &[Tensor],
    golden_vectors: &[GoldenVector],
    toolchain_id: &str,
    recipe_hash: &str,
    weights_source_sha256: &str,
) -> String {
    let mut canonical = String::new();

    canonical.push_str(&format!("model|{model_id}\n"));
    canonical.push_str(&format!(
        "input|{}|{}\n",
        input_spec.length, input_spec.channels
    ));
    canonical.push_str(&format!(
        "output|{}|{}\n",
        output_spec.classes,
        output_spec.labels.join(",")
    ));
    for layer in layers {
        canonical.push_str(&format!("layer|{layer:?}\n"));
    }
    for tensor in tensors {
        canonical.push_str(&format!(
            "tensor|{}|{:?}|{}\n",
            tensor.id,
            tensor.shape,
            sha256_bytes(&f32_to_le_bytes(&tensor.data))
        ));
    }
    for golden in golden_vectors {
        canonical.push_str(&format!(
            "golden|{}|{}\n",
            sha256_bytes(&f32_to_le_bytes(&golden.input)),
            sha256_bytes(&f32_to_le_bytes(&golden.expected))
        ));
    }
    canonical.push_str(&format!(
        "evidence|{toolchain_id}|{recipe_hash}|{weights_source_sha256}\n"
    ));

    sha256_text(&canonical)
}

fn f32_to_le_bytes(values: &[f32]) -> Vec<u8> {
    let mut bytes = Vec::with_capacity(values.len() * 4);
    for value in values {
        bytes.extend_from_slice(&value.to_le_bytes());
    }
    bytes
}

fn sha256_text(value: &str) -> String {
    sha256_bytes(value.as_bytes())
}

fn sha256_bytes(value: &[u8]) -> String {
    let digest = Sha256::digest(value);
    let mut encoded = String::with_capacity(digest.len() * 2);
    for byte in digest {
        encoded.push_str(&format!("{byte:02x}"));
    }
    encoded
}

pub fn pipeline_description() -> &'static str {
    "weights-intake -> arch-validate -> deterministic-compile -> golden-embed -> package-verify"
}

#[cfg(test)]
mod tests {
    use super::*;

    fn mlp_input() -> ModelCompilationInput {
        ModelCompilationInput {
            model_id: "TEST-MLP".to_string(),
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
                    data: vec![0.1, -0.2, 0.3, 0.05, -0.1, 0.4, 0.2, -0.3],
                },
                Tensor {
                    id: "dense.bias".to_string(),
                    shape: vec![2],
                    data: vec![0.0, 0.1],
                },
            ],
            toolchain_id: "rust-1.87.0+locked".to_string(),
            build_recipe: "test-recipe".to_string(),
            weights_source_sha256: "0".repeat(64),
            golden_seed: 42,
            golden_vector_count: 4,
        }
    }

    #[test]
    fn compiles_a_valid_package_with_golden_vectors() {
        let package = compile_model_package(mlp_input()).expect("compile should succeed");
        assert_eq!(package.golden_vectors.len(), 4);
        assert!(package.validate().is_ok());
    }

    #[test]
    fn compiles_deterministically_regardless_of_tensor_order() {
        let mut reordered = mlp_input();
        reordered.tensors.reverse();

        let first = compile_model_package(mlp_input()).expect("first compile");
        let second = compile_model_package(reordered).expect("second compile");

        assert_eq!(first.evidence.package_sha256, second.evidence.package_sha256);
    }

    #[test]
    fn same_seed_produces_identical_golden_vectors() {
        let a = compile_model_package(mlp_input()).unwrap();
        let b = compile_model_package(mlp_input()).unwrap();
        for (va, vb) in a.golden_vectors.iter().zip(b.golden_vectors.iter()) {
            assert_eq!(va.input, vb.input);
            assert_eq!(va.expected, vb.expected);
        }
    }

    #[test]
    fn different_seed_produces_different_inputs() {
        let mut input_a = mlp_input();
        input_a.golden_seed = 1;
        let mut input_b = mlp_input();
        input_b.golden_seed = 2;

        let a = compile_model_package(input_a).unwrap();
        let b = compile_model_package(input_b).unwrap();
        assert_ne!(a.golden_vectors[0].input, b.golden_vectors[0].input);
    }

    #[test]
    fn round_trips_a_hand_built_safetensors_fixture() {
        // Build a minimal safetensors blob in memory: one F32 tensor "w" of shape [2].
        let data: Vec<f32> = vec![1.5, -2.5];
        let mut raw = Vec::new();
        for value in &data {
            raw.extend_from_slice(&value.to_le_bytes());
        }
        let header = serde_json::json!({
            "w": { "dtype": "F32", "shape": [2], "data_offsets": [0, raw.len()] }
        });
        let header_bytes = serde_json::to_vec(&header).unwrap();
        let mut blob = Vec::new();
        blob.extend_from_slice(&(header_bytes.len() as u64).to_le_bytes());
        blob.extend_from_slice(&header_bytes);
        blob.extend_from_slice(&raw);

        let manifest = ArchitectureManifest {
            tensor_map: vec![("w".to_string(), "our.w".to_string())],
        };
        let imported = import_safetensors(&blob, &manifest).expect("import should succeed");

        assert_eq!(imported.tensors.len(), 1);
        assert_eq!(imported.tensors[0].id, "our.w");
        assert_eq!(imported.tensors[0].shape, vec![2]);
        assert_eq!(imported.tensors[0].data, data);
    }

    #[test]
    fn rejects_non_f32_safetensors_dtype() {
        let header = serde_json::json!({
            "w": { "dtype": "BF16", "shape": [2], "data_offsets": [0, 4] }
        });
        let header_bytes = serde_json::to_vec(&header).unwrap();
        let mut blob = Vec::new();
        blob.extend_from_slice(&(header_bytes.len() as u64).to_le_bytes());
        blob.extend_from_slice(&header_bytes);
        blob.extend_from_slice(&[0u8; 4]);

        let manifest = ArchitectureManifest {
            tensor_map: vec![("w".to_string(), "our.w".to_string())],
        };
        let error = import_safetensors(&blob, &manifest).expect_err("BF16 must be rejected");
        assert!(error.to_string().contains("F32"));
    }

    #[test]
    fn rejects_truncated_safetensors_file() {
        let manifest = ArchitectureManifest {
            tensor_map: vec![],
        };
        let error = import_safetensors(&[1, 2, 3], &manifest).expect_err("too short");
        assert!(error.to_string().contains("too short"));
    }
}
