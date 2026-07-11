#![forbid(unsafe_code)]

use std::fs;
use std::path::{Path, PathBuf};

use mdux_core::{MduxResult, ValidationError};
use mdux_ml_authoring::{
    ArchitectureManifest, ModelCompilationInput, compile_model_package, fingerprint_weights_file,
    import_safetensors, pipeline_description,
};
use mdux_ml_schema::{GoldenVector, InputSpec, Layer, ModelPackage, OutputSpec, Tensor};
use serde::{Deserialize, Serialize};
use sha2::{Digest, Sha256};

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct BakeSummary {
    pub package_sha256: String,
    pub layer_count: usize,
    pub tensor_count: usize,
    pub param_count: usize,
    pub golden_vector_count: usize,
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct VerificationSummary {
    pub package_sha256: String,
    pub package_bytes_verified: usize,
    pub report_bytes_verified: usize,
}

#[derive(Clone, Debug)]
pub struct BakeArtifacts {
    pub package: ModelPackage,
    pub package_bytes: Vec<u8>,
    pub report_bytes: Vec<u8>,
    pub summary: BakeSummary,
}

#[derive(Clone, Copy, Debug)]
pub struct CliInvocation<'a> {
    pub recipe_path: &'a Path,
    pub package_output_path: &'a Path,
    pub report_output_path: &'a Path,
}

pub fn bake(invocation: CliInvocation<'_>) -> MduxResult<BakeSummary> {
    let artifacts = compile_recipe(invocation.recipe_path)?;
    write_bytes(invocation.package_output_path, &artifacts.package_bytes)?;
    write_bytes(invocation.report_output_path, &artifacts.report_bytes)?;
    Ok(artifacts.summary)
}

pub fn verify(invocation: CliInvocation<'_>) -> MduxResult<VerificationSummary> {
    let artifacts = compile_recipe(invocation.recipe_path)?;
    let existing_package = fs::read(invocation.package_output_path).map_err(|error| {
        ValidationError::new(format!(
            "failed to read baked package {}: {error}",
            invocation.package_output_path.display()
        ))
    })?;
    let existing_report = fs::read(invocation.report_output_path).map_err(|error| {
        ValidationError::new(format!(
            "failed to read bake report {}: {error}",
            invocation.report_output_path.display()
        ))
    })?;

    if existing_package != artifacts.package_bytes {
        return Err(ValidationError::new(
            "baked model package content does not match a fresh deterministic rebuild",
        ));
    }
    if existing_report != artifacts.report_bytes {
        return Err(ValidationError::new(
            "bake report content does not match a fresh deterministic rebuild",
        ));
    }

    Ok(VerificationSummary {
        package_sha256: artifacts.summary.package_sha256,
        package_bytes_verified: existing_package.len(),
        report_bytes_verified: existing_report.len(),
    })
}

/// Reads a `safetensors` file plus a small tensor-map TOML and prints a ready-to-paste
/// `[weights]` recipe fragment (source, path, sha256, tensor_map) to stdout. This is the
/// Phase-1 Hugging Face demonstrator entry point (ADR-017 §2): it validates the referenced
/// tensors exist and are `F32`, but never writes committed evidence and never runs in CI —
/// only `bake`/`verify` do that, against a recipe a human has reviewed and committed.
pub fn import(safetensors_path: &Path, tensor_map_path: &Path) -> MduxResult<String> {
    let bytes = fs::read(safetensors_path).map_err(|error| {
        ValidationError::new(format!(
            "failed to read safetensors file {}: {error}",
            safetensors_path.display()
        ))
    })?;
    let tensor_map_text = fs::read_to_string(tensor_map_path).map_err(|error| {
        ValidationError::new(format!(
            "failed to read tensor map {}: {error}",
            tensor_map_path.display()
        ))
    })?;
    let tensor_map_document: TensorMapDocument =
        toml::from_str(&tensor_map_text).map_err(|error| {
            ValidationError::new(format!(
                "failed to parse tensor map {}: {error}",
                tensor_map_path.display()
            ))
        })?;

    let manifest = ArchitectureManifest {
        tensor_map: tensor_map_document
            .tensor
            .iter()
            .map(|entry| (entry.safetensors_name.clone(), entry.id.clone()))
            .collect(),
    };
    let imported = import_safetensors(&bytes, &manifest)?;

    let mut fragment = String::new();
    fragment.push_str("[weights]\n");
    fragment.push_str("source = \"safetensors\"\n");
    fragment.push_str(&format!(
        "path = {:?}\n",
        safetensors_path.display().to_string()
    ));
    fragment.push_str(&format!("sha256 = {:?}\n", imported.source_sha256));
    for (safetensors_name, id) in &manifest.tensor_map {
        fragment.push_str("\n[[weights.tensor_map]]\n");
        fragment.push_str(&format!("safetensors_name = {safetensors_name:?}\n"));
        fragment.push_str(&format!("id = {id:?}\n"));
    }

    Ok(fragment)
}

pub fn compile_recipe(recipe_path: impl AsRef<Path>) -> MduxResult<BakeArtifacts> {
    let recipe_path = recipe_path.as_ref();
    let recipe_text = fs::read_to_string(recipe_path).map_err(|error| {
        ValidationError::new(format!(
            "failed to read bake recipe {}: {error}",
            recipe_path.display()
        ))
    })?;
    let recipe: BakeRecipe = toml::from_str(&recipe_text).map_err(|error| {
        ValidationError::new(format!(
            "failed to parse bake recipe {}: {error}",
            recipe_path.display()
        ))
    })?;

    let recipe_dir = recipe_path
        .parent()
        .ok_or_else(|| ValidationError::new("bake recipe path must have a parent directory"))?;

    let (tensors, weights_source_sha256) = resolve_weights(&recipe.weights, recipe_dir)?;
    let layers: Vec<Layer> = recipe.layers.iter().map(Layer::from).collect();

    let compilation_input = ModelCompilationInput {
        model_id: recipe.model_id.clone(),
        input_spec: InputSpec {
            length: recipe.input.length,
            channels: recipe.input.channels,
        },
        output_spec: OutputSpec {
            classes: recipe.output.classes,
            labels: recipe.output.labels.clone(),
        },
        layers,
        tensors,
        toolchain_id: recipe.toolchain_id.clone(),
        build_recipe: recipe_text.clone(),
        weights_source_sha256,
        golden_seed: recipe.golden_seed,
        golden_vector_count: recipe.golden_vector_count,
    };

    let package = compile_model_package(compilation_input)?;

    let recipe_sha256 = sha256_text(&recipe_text);
    let param_count: usize = package.tensors.iter().map(|tensor| tensor.data.len()).sum();
    let report_document = BakeReportDocument {
        report_kind: "mdux-ml-baker-report".to_string(),
        model_id: package.model_id.clone(),
        labels: package.output_spec.labels.clone(),
        package_sha256: package.evidence.package_sha256.clone(),
        recipe_sha256,
        weights_source_sha256: package.evidence.weights_source_sha256.clone(),
        layer_count: package.layers.len(),
        tensor_count: package.tensors.len(),
        param_count,
        golden_vector_count: package.golden_vectors.len(),
        pipeline: pipeline_description().to_string(),
    };

    let package_document = ModelPackageDocument::from(&package);
    let package_bytes = to_pretty_json(&package_document)?;
    let report_bytes = to_pretty_json(&report_document)?;
    let summary = BakeSummary {
        package_sha256: package.evidence.package_sha256.clone(),
        layer_count: package.layers.len(),
        tensor_count: package.tensors.len(),
        param_count,
        golden_vector_count: package.golden_vectors.len(),
    };

    Ok(BakeArtifacts {
        package,
        package_bytes,
        report_bytes,
        summary,
    })
}

fn resolve_weights(weights: &WeightsRecipe, recipe_dir: &Path) -> MduxResult<(Vec<Tensor>, String)> {
    match weights {
        WeightsRecipe::Inline { tensors } => {
            let tensors: Vec<Tensor> = tensors
                .iter()
                .map(|entry| Tensor {
                    id: entry.id.clone(),
                    shape: entry.shape.clone(),
                    data: entry.data.clone(),
                })
                .collect();
            let source_sha256 = hash_inline_tensors(&tensors);
            Ok((tensors, source_sha256))
        }
        WeightsRecipe::Safetensors {
            path,
            sha256,
            tensor_map,
        } => {
            let resolved_path = resolve_path(recipe_dir, path);
            let fingerprint = fingerprint_weights_file(&resolved_path)?;
            if &fingerprint.sha256 != sha256 {
                return Err(ValidationError::new(format!(
                    "safetensors file {} digest {} does not match recipe-declared {sha256}",
                    resolved_path.display(),
                    fingerprint.sha256
                )));
            }
            let bytes = fs::read(&resolved_path).map_err(|error| {
                ValidationError::new(format!(
                    "failed to read safetensors file {}: {error}",
                    resolved_path.display()
                ))
            })?;
            let manifest = ArchitectureManifest {
                tensor_map: tensor_map
                    .iter()
                    .map(|entry| (entry.safetensors_name.clone(), entry.id.clone()))
                    .collect(),
            };
            let imported = import_safetensors(&bytes, &manifest)?;
            Ok((imported.tensors, imported.source_sha256))
        }
    }
}

/// Hashes inline tensors sorted by id, not recipe declaration order: `compile_model_package`
/// itself sorts tensors by id before computing `package_sha256`, so reordering
/// `[[weights.tensors]]` entries in a recipe must not change `weights_source_sha256` either --
/// otherwise the "reordering input tensors never changes the baked digest" guarantee would only
/// hold for the package hash, not the weights-source hash folded into it.
fn hash_inline_tensors(tensors: &[Tensor]) -> String {
    let mut sorted: Vec<&Tensor> = tensors.iter().collect();
    sorted.sort_by(|left, right| left.id.cmp(&right.id));

    let mut canonical = String::new();
    for tensor in sorted {
        canonical.push_str(&format!("tensor|{}|{:?}\n", tensor.id, tensor.shape));
        for value in &tensor.data {
            canonical.push_str(&format!("{}\n", value.to_bits()));
        }
    }
    sha256_text(&canonical)
}

fn resolve_path(base: &Path, value: &str) -> PathBuf {
    let candidate = Path::new(value);
    if candidate.is_absolute() {
        candidate.to_path_buf()
    } else {
        base.join(candidate)
    }
}

fn write_bytes(path: &Path, bytes: &[u8]) -> MduxResult<()> {
    if let Some(parent) = path.parent() {
        if !parent.as_os_str().is_empty() {
            fs::create_dir_all(parent).map_err(|error| {
                ValidationError::new(format!(
                    "failed to create output directory {}: {error}",
                    parent.display()
                ))
            })?;
        }
    }
    fs::write(path, bytes).map_err(|error| {
        ValidationError::new(format!("failed to write {}: {error}", path.display()))
    })
}

fn to_pretty_json<T: Serialize>(value: &T) -> MduxResult<Vec<u8>> {
    let mut bytes = serde_json::to_vec_pretty(value)
        .map_err(|error| ValidationError::new(format!("failed to serialize JSON: {error}")))?;
    bytes.push(b'\n');
    Ok(bytes)
}

fn sha256_text(value: &str) -> String {
    let digest = Sha256::digest(value.as_bytes());
    let mut encoded = String::with_capacity(digest.len() * 2);
    for byte in digest {
        encoded.push_str(&format!("{byte:02x}"));
    }
    encoded
}

// ---- Recipe (input) schema ----

#[derive(Clone, Debug, Deserialize)]
#[serde(deny_unknown_fields)]
struct BakeRecipe {
    model_id: String,
    toolchain_id: String,
    golden_seed: u64,
    golden_vector_count: usize,
    input: InputRecipe,
    output: OutputRecipe,
    layers: Vec<LayerRecipe>,
    weights: WeightsRecipe,
}

#[derive(Clone, Debug, Deserialize)]
#[serde(deny_unknown_fields)]
struct InputRecipe {
    length: u16,
    channels: u16,
}

#[derive(Clone, Debug, Deserialize)]
#[serde(deny_unknown_fields)]
struct OutputRecipe {
    classes: u16,
    labels: Vec<String>,
}

#[derive(Clone, Debug, Deserialize)]
#[serde(tag = "kind", rename_all = "snake_case", deny_unknown_fields)]
enum LayerRecipe {
    Conv1d {
        weights_id: String,
        bias_id: String,
        in_channels: u16,
        out_channels: u16,
        kernel: u16,
        stride: u16,
        padding: u16,
    },
    MaxPool1d {
        window: u16,
        stride: u16,
    },
    AvgPool1d {
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

impl From<&LayerRecipe> for Layer {
    fn from(value: &LayerRecipe) -> Self {
        match value.clone() {
            LayerRecipe::Conv1d {
                weights_id,
                bias_id,
                in_channels,
                out_channels,
                kernel,
                stride,
                padding,
            } => Layer::Conv1D {
                weights_id,
                bias_id,
                in_channels,
                out_channels,
                kernel,
                stride,
                padding,
            },
            LayerRecipe::MaxPool1d { window, stride } => Layer::MaxPool1D { window, stride },
            LayerRecipe::AvgPool1d { window, stride } => Layer::AvgPool1D { window, stride },
            LayerRecipe::Flatten => Layer::Flatten,
            LayerRecipe::Dense {
                weights_id,
                bias_id,
                in_features,
                out_features,
            } => Layer::Dense {
                weights_id,
                bias_id,
                in_features,
                out_features,
            },
            LayerRecipe::Relu => Layer::Relu,
            LayerRecipe::Sigmoid => Layer::Sigmoid,
            LayerRecipe::Softmax => Layer::Softmax,
        }
    }
}

#[derive(Clone, Debug, Deserialize)]
#[serde(tag = "source", rename_all = "snake_case", deny_unknown_fields)]
enum WeightsRecipe {
    Inline {
        tensors: Vec<InlineTensorRecipe>,
    },
    Safetensors {
        path: String,
        sha256: String,
        tensor_map: Vec<TensorMapEntry>,
    },
}

#[derive(Clone, Debug, Deserialize)]
#[serde(deny_unknown_fields)]
struct InlineTensorRecipe {
    id: String,
    shape: Vec<u32>,
    data: Vec<f32>,
}

#[derive(Clone, Debug, Deserialize)]
#[serde(deny_unknown_fields)]
struct TensorMapEntry {
    safetensors_name: String,
    id: String,
}

#[derive(Clone, Debug, Deserialize)]
#[serde(deny_unknown_fields)]
struct TensorMapDocument {
    tensor: Vec<TensorMapEntry>,
}

// ---- Evidence (output) documents ----

#[derive(Clone, Debug, Serialize)]
struct BakeReportDocument {
    report_kind: String,
    model_id: String,
    labels: Vec<String>,
    package_sha256: String,
    recipe_sha256: String,
    weights_source_sha256: String,
    layer_count: usize,
    tensor_count: usize,
    param_count: usize,
    golden_vector_count: usize,
    pipeline: String,
}

#[derive(Clone, Debug, Serialize)]
struct ModelPackageDocument {
    model_id: String,
    dtype: &'static str,
    input_spec: InputSpecDocument,
    output_spec: OutputSpecDocument,
    layers: Vec<LayerDocument>,
    tensors: Vec<TensorDocument>,
    golden_vectors: Vec<GoldenVectorDocument>,
    evidence: EvidenceDocument,
}

#[derive(Clone, Debug, Serialize)]
struct InputSpecDocument {
    length: u16,
    channels: u16,
}

#[derive(Clone, Debug, Serialize)]
struct OutputSpecDocument {
    classes: u16,
    labels: Vec<String>,
}

#[derive(Clone, Debug, Serialize)]
#[serde(tag = "kind", rename_all = "snake_case")]
enum LayerDocument {
    Conv1d {
        weights_id: String,
        bias_id: String,
        in_channels: u16,
        out_channels: u16,
        kernel: u16,
        stride: u16,
        padding: u16,
    },
    MaxPool1d {
        window: u16,
        stride: u16,
    },
    AvgPool1d {
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

impl From<&Layer> for LayerDocument {
    fn from(value: &Layer) -> Self {
        match value.clone() {
            Layer::Conv1D {
                weights_id,
                bias_id,
                in_channels,
                out_channels,
                kernel,
                stride,
                padding,
            } => LayerDocument::Conv1d {
                weights_id,
                bias_id,
                in_channels,
                out_channels,
                kernel,
                stride,
                padding,
            },
            Layer::MaxPool1D { window, stride } => LayerDocument::MaxPool1d { window, stride },
            Layer::AvgPool1D { window, stride } => LayerDocument::AvgPool1d { window, stride },
            Layer::Flatten => LayerDocument::Flatten,
            Layer::Dense {
                weights_id,
                bias_id,
                in_features,
                out_features,
            } => LayerDocument::Dense {
                weights_id,
                bias_id,
                in_features,
                out_features,
            },
            Layer::Relu => LayerDocument::Relu,
            Layer::Sigmoid => LayerDocument::Sigmoid,
            Layer::Softmax => LayerDocument::Softmax,
        }
    }
}

#[derive(Clone, Debug, Serialize)]
struct TensorDocument {
    id: String,
    shape: Vec<u32>,
    data: Vec<f32>,
}

impl From<&Tensor> for TensorDocument {
    fn from(value: &Tensor) -> Self {
        Self {
            id: value.id.clone(),
            shape: value.shape.clone(),
            data: value.data.clone(),
        }
    }
}

#[derive(Clone, Debug, Serialize)]
struct GoldenVectorDocument {
    input: Vec<f32>,
    expected: Vec<f32>,
}

impl From<&GoldenVector> for GoldenVectorDocument {
    fn from(value: &GoldenVector) -> Self {
        Self {
            input: value.input.clone(),
            expected: value.expected.clone(),
        }
    }
}

#[derive(Clone, Debug, Serialize)]
struct EvidenceDocument {
    package_sha256: String,
    toolchain_id: String,
    build_recipe_sha256: String,
    weights_source_sha256: String,
}

impl From<&ModelPackage> for ModelPackageDocument {
    fn from(package: &ModelPackage) -> Self {
        Self {
            model_id: package.model_id.clone(),
            dtype: "F32",
            input_spec: InputSpecDocument {
                length: package.input_spec.length,
                channels: package.input_spec.channels,
            },
            output_spec: OutputSpecDocument {
                classes: package.output_spec.classes,
                labels: package.output_spec.labels.clone(),
            },
            layers: package.layers.iter().map(LayerDocument::from).collect(),
            tensors: package.tensors.iter().map(TensorDocument::from).collect(),
            golden_vectors: package
                .golden_vectors
                .iter()
                .map(GoldenVectorDocument::from)
                .collect(),
            evidence: EvidenceDocument {
                package_sha256: package.evidence.package_sha256.clone(),
                toolchain_id: package.evidence.toolchain_id.clone(),
                build_recipe_sha256: package.evidence.build_recipe_sha256.clone(),
                weights_source_sha256: package.evidence.weights_source_sha256.clone(),
            },
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::time::{SystemTime, UNIX_EPOCH};

    fn fixture_recipe_path() -> PathBuf {
        PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("fixtures/eeg-demo.toml")
    }

    fn target_output_paths() -> (PathBuf, PathBuf) {
        let unique = SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .expect("system time should be after unix epoch")
            .as_nanos();
        let base = PathBuf::from(env!("CARGO_MANIFEST_DIR"))
            .join("../../target/mdux-ml-baker-tests")
            .join(format!("run-{unique}"));
        (
            base.join("eeg-demo.package.json"),
            base.join("eeg-demo.report.json"),
        )
    }

    #[test]
    fn bakes_and_verifies_the_eeg_demo_fixture() {
        let recipe_path = fixture_recipe_path();
        let (package_output_path, report_output_path) = target_output_paths();
        let invocation = CliInvocation {
            recipe_path: &recipe_path,
            package_output_path: &package_output_path,
            report_output_path: &report_output_path,
        };

        let summary = bake(invocation).expect("fixture recipe should bake successfully");
        assert!(summary.tensor_count > 0);
        assert!(summary.golden_vector_count > 0);

        let verification = verify(invocation).expect("fixture bake should verify successfully");
        assert_eq!(verification.package_sha256, summary.package_sha256);

        fs::remove_dir_all(package_output_path.parent().unwrap())
            .expect("temporary output directory should be removable");
    }

    #[test]
    fn rebaking_is_byte_identical() {
        let first = compile_recipe(fixture_recipe_path()).expect("first compile");
        let second = compile_recipe(fixture_recipe_path()).expect("second compile");
        assert_eq!(first.package_bytes, second.package_bytes);
        assert_eq!(first.report_bytes, second.report_bytes);
    }

    #[test]
    fn hash_inline_tensors_is_independent_of_declaration_order() {
        let forward = vec![
            Tensor {
                id: "a".to_string(),
                shape: vec![1],
                data: vec![1.0],
            },
            Tensor {
                id: "b".to_string(),
                shape: vec![1],
                data: vec![2.0],
            },
        ];
        let mut reversed = forward.clone();
        reversed.reverse();

        assert_eq!(hash_inline_tensors(&forward), hash_inline_tensors(&reversed));
    }

    /// The whole point of ADR-017's demonstrator story is that the baked model must actually
    /// work, not just validate structurally. A near-isoelectric window (total spectral energy
    /// near zero) must classify BURST_SUPPRESSION; a moderate-energy window (mid threshold band)
    /// must classify ADEQUATE; a high-energy, broadband window must classify AWAKE.
    #[test]
    fn baked_model_actually_detects_the_depth_of_anesthesia_state() {
        use mdux_ml_runtime::Classifier1D;

        let artifacts = compile_recipe(fixture_recipe_path()).expect("compile should succeed");
        let runtime = Classifier1D::<128, 4>::new(&artifacts.package)
            .expect("golden self-test should pass");

        // Total energy ~2.0 (< 30 threshold): near-isoelectric EEG.
        let mut burst_window = vec![0.0f32; 64];
        burst_window[32] = 2.0;
        let burst_prediction = runtime.predict(&burst_window).expect("predict should succeed");
        assert_eq!(burst_prediction.class, 2, "near-isoelectric window must read BURST_SUPPRESSION");

        // Total energy = 40.0 (between the 30 and 50 thresholds): typical anesthetized spectrum.
        let mut adequate_window = vec![0.0f32; 64];
        for value in adequate_window.iter_mut().take(40) {
            *value = 1.0;
        }
        let adequate_prediction = runtime
            .predict(&adequate_window)
            .expect("predict should succeed");
        assert_eq!(adequate_prediction.class, 1, "mid-energy window must read ADEQUATE");

        // Total energy = 64.0 (> 50 threshold): broadband high-energy activity.
        let awake_window = vec![1.0f32; 64];
        let awake_prediction = runtime.predict(&awake_window).expect("predict should succeed");
        assert_eq!(awake_prediction.class, 0, "high-energy window must read AWAKE");
    }
}
