//! Build-script helper embedding a committed ML model package
//! (`generated/models/<id>/package.json`, ADR-017) as generated Rust source — the "weights are
//! data" mechanism: swapping which `package.json` a `ModelPackage::new(..).compile()` call
//! points at (Hugging Face demonstrator vs. a manufacturer's own clinically-qualified
//! production weights) changes zero application source, the same doctrine `MeduiScreen`
//! applies to `.medui` files and the facade's own `build.rs` applies to the standard text/image
//! packages.
//!
//! ```no_run
//! fn main() -> Result<(), Box<dyn std::error::Error>> {
//!     mdux_build::ModelPackage::new("../../generated/models/eeg-demo/package.json").compile()
//! }
//! ```
//!
//! Pair this with `mdux::include_model!()` in the crate's `src/` to bring the generated
//! `medui_model` module into scope, exposing `medui_model::model() -> mdux::ModelPackage`.

use std::fmt::Write as _;
use std::{
    env, fs,
    path::{Path, PathBuf},
};

use serde::Deserialize;

use crate::DynError;

/// Builder for compiling one committed model `package.json` into the generated Rust module
/// consumed by `mdux::include_model!()`.
pub struct ModelPackage {
    package_json: PathBuf,
}

impl ModelPackage {
    /// `package_json` is resolved relative to `CARGO_MANIFEST_DIR` of the calling build script.
    pub fn new(package_json: impl AsRef<Path>) -> Self {
        Self {
            package_json: package_json.as_ref().to_path_buf(),
        }
    }

    /// Parses and re-emits the committed model package as `$OUT_DIR/mdux_ml_model.rs`, and
    /// emits `cargo:rerun-if-changed` for the source file. Does not re-validate or re-bake —
    /// that already happened when `tools/mdux-ml-baker` produced (and CI verified) the
    /// committed `package.json`; this step only transcribes it into static Rust data.
    pub fn compile(self) -> Result<(), DynError> {
        let manifest_dir = PathBuf::from(env::var("CARGO_MANIFEST_DIR")?);
        let package_path = manifest_dir.join(&self.package_json);
        let out_dir = PathBuf::from(env::var("OUT_DIR")?);
        let generated_path = out_dir.join("mdux_ml_model.rs");

        println!("cargo:rerun-if-changed={}", package_path.display());

        let document_text = fs::read_to_string(&package_path).map_err(|error| {
            format!(
                "failed to read model package {}: {error}",
                package_path.display()
            )
        })?;
        let rendered = parse_and_render(&document_text, &package_path.display().to_string())?;

        fs::create_dir_all(&out_dir)?;
        fs::write(&generated_path, rendered).map_err(|error| {
            format!(
                "failed to write generated model module {}: {error}",
                generated_path.display()
            )
        })?;

        Ok(())
    }
}

/// Parses a model package document and renders it into `$OUT_DIR/mdux_ml_model.rs` source, or
/// rejects a non-`F32` dtype (ADR-017 §5). `source_label` is only used for error messages, so
/// this half of the pipeline is fully testable without touching the filesystem or environment.
fn parse_and_render(document_text: &str, source_label: &str) -> Result<String, DynError> {
    let document: ModelPackageDocument = serde_json::from_str(document_text)
        .map_err(|error| format!("failed to parse model package {source_label}: {error}"))?;
    if document.dtype != "F32" {
        return Err(format!(
            "model package {source_label} declares dtype {:?}; only F32 is supported in v1 (ADR-017 \u{a7}5)",
            document.dtype
        )
        .into());
    }
    Ok(render_model_package(&document))
}

#[derive(Clone, Debug, Deserialize)]
#[serde(deny_unknown_fields)]
struct ModelPackageDocument {
    model_id: String,
    dtype: String,
    input_spec: InputSpecDocument,
    output_spec: OutputSpecDocument,
    layers: Vec<LayerDocument>,
    tensors: Vec<TensorDocument>,
    golden_vectors: Vec<GoldenVectorDocument>,
    evidence: EvidenceDocument,
}

#[derive(Clone, Debug, Deserialize)]
#[serde(deny_unknown_fields)]
struct InputSpecDocument {
    length: u16,
    channels: u16,
}

#[derive(Clone, Debug, Deserialize)]
#[serde(deny_unknown_fields)]
struct OutputSpecDocument {
    classes: u16,
    labels: Vec<String>,
}

#[derive(Clone, Debug, Deserialize)]
#[serde(tag = "kind", rename_all = "snake_case", deny_unknown_fields)]
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

#[derive(Clone, Debug, Deserialize)]
#[serde(deny_unknown_fields)]
struct TensorDocument {
    id: String,
    shape: Vec<u32>,
    data: Vec<f32>,
}

#[derive(Clone, Debug, Deserialize)]
#[serde(deny_unknown_fields)]
struct GoldenVectorDocument {
    input: Vec<f32>,
    expected: Vec<f32>,
}

#[derive(Clone, Debug, Deserialize)]
#[serde(deny_unknown_fields)]
struct EvidenceDocument {
    package_sha256: String,
    toolchain_id: String,
    build_recipe_sha256: String,
    weights_source_sha256: String,
}

fn render_model_package(document: &ModelPackageDocument) -> String {
    let mut out = String::new();
    let _ = writeln!(out, "pub fn model() -> ::mdux::ModelPackage {{");
    let _ = writeln!(out, "    ::mdux::ModelPackage {{");
    let _ = writeln!(out, "        model_id: {},", rust_string(&document.model_id));
    let _ = writeln!(out, "        dtype: ::mdux::MlDtype::F32,");
    let _ = writeln!(
        out,
        "        input_spec: ::mdux::MlInputSpec {{ length: {}, channels: {} }},",
        document.input_spec.length, document.input_spec.channels
    );
    let _ = writeln!(
        out,
        "        output_spec: ::mdux::MlOutputSpec {{ classes: {}, labels: vec![{}] }},",
        document.output_spec.classes,
        render_string_vec(&document.output_spec.labels),
    );
    let _ = writeln!(out, "        layers: vec![");
    for layer in &document.layers {
        let _ = writeln!(out, "            {},", render_layer(layer));
    }
    let _ = writeln!(out, "        ],");
    let _ = writeln!(out, "        tensors: vec![");
    for tensor in &document.tensors {
        let _ = writeln!(
            out,
            "            ::mdux::MlTensor {{ id: {}, shape: vec![{}], data: vec![{}] }},",
            rust_string(&tensor.id),
            render_u32_vec(&tensor.shape),
            render_f32_vec(&tensor.data),
        );
    }
    let _ = writeln!(out, "        ],");
    let _ = writeln!(out, "        golden_vectors: vec![");
    for golden in &document.golden_vectors {
        let _ = writeln!(
            out,
            "            ::mdux::MlGoldenVector {{ input: vec![{}], expected: vec![{}] }},",
            render_f32_vec(&golden.input),
            render_f32_vec(&golden.expected),
        );
    }
    let _ = writeln!(out, "        ],");
    let _ = writeln!(
        out,
        "        evidence: ::mdux::MlDeterminismEvidence {{ package_sha256: {}, toolchain_id: {}, build_recipe_sha256: {}, weights_source_sha256: {} }},",
        rust_string(&document.evidence.package_sha256),
        rust_string(&document.evidence.toolchain_id),
        rust_string(&document.evidence.build_recipe_sha256),
        rust_string(&document.evidence.weights_source_sha256),
    );
    let _ = writeln!(out, "    }}");
    let _ = writeln!(out, "}}");
    out
}

fn render_layer(layer: &LayerDocument) -> String {
    match layer {
        LayerDocument::Conv1d {
            weights_id,
            bias_id,
            in_channels,
            out_channels,
            kernel,
            stride,
            padding,
        } => format!(
            "::mdux::MlLayer::Conv1D {{ weights_id: {}, bias_id: {}, in_channels: {in_channels}, out_channels: {out_channels}, kernel: {kernel}, stride: {stride}, padding: {padding} }}",
            rust_string(weights_id),
            rust_string(bias_id),
        ),
        LayerDocument::MaxPool1d { window, stride } => {
            format!("::mdux::MlLayer::MaxPool1D {{ window: {window}, stride: {stride} }}")
        }
        LayerDocument::AvgPool1d { window, stride } => {
            format!("::mdux::MlLayer::AvgPool1D {{ window: {window}, stride: {stride} }}")
        }
        LayerDocument::Flatten => "::mdux::MlLayer::Flatten".to_string(),
        LayerDocument::Dense {
            weights_id,
            bias_id,
            in_features,
            out_features,
        } => format!(
            "::mdux::MlLayer::Dense {{ weights_id: {}, bias_id: {}, in_features: {in_features}, out_features: {out_features} }}",
            rust_string(weights_id),
            rust_string(bias_id),
        ),
        LayerDocument::Relu => "::mdux::MlLayer::Relu".to_string(),
        LayerDocument::Sigmoid => "::mdux::MlLayer::Sigmoid".to_string(),
        LayerDocument::Softmax => "::mdux::MlLayer::Softmax".to_string(),
    }
}

fn render_string_vec(values: &[String]) -> String {
    values
        .iter()
        .map(|value| format!("{value:?}.to_string()"))
        .collect::<Vec<_>>()
        .join(", ")
}

fn render_u32_vec(values: &[u32]) -> String {
    values
        .iter()
        .map(|value| format!("{value}u32"))
        .collect::<Vec<_>>()
        .join(", ")
}

/// Renders each value as `f32::from_bits(<u32>)` rather than a literal float token: `{:?}`
/// formatting produces `inf`/`NaN`, which aren't valid Rust float literals, and even for finite
/// values a decimal round-trip isn't guaranteed to reproduce the exact bit pattern (a NaN's
/// payload bits in particular). Bit-exact encoding is what the "byte-exact evidence -> generated
/// Rust" goal (ADR-017) actually requires.
fn render_f32_vec(values: &[f32]) -> String {
    values
        .iter()
        .map(|value| format!("f32::from_bits({}u32)", value.to_bits()))
        .collect::<Vec<_>>()
        .join(", ")
}

fn rust_string(value: &str) -> String {
    format!("{value:?}.to_string()")
}

#[cfg(test)]
mod tests {
    use super::*;

    /// Parses and renders the real committed evidence (`generated/models/eeg-demo/package.json`,
    /// baked and CI-verified by `tools/mdux-ml-baker`) exactly as `compile()` does, proving the
    /// parse-then-render half of the pipeline works end-to-end against the artifact this
    /// repository actually ships — not just a hand-built fixture. `compile()` itself is not unit
    /// tested (it reads `CARGO_MANIFEST_DIR`/`OUT_DIR` the way a real build script does); its
    /// contract is exercised by an application's `build.rs` instead, mirroring `MeduiScreen`.
    #[test]
    fn renders_the_committed_eeg_demo_package_into_valid_looking_rust_source() {
        let package_path = PathBuf::from(env!("CARGO_MANIFEST_DIR"))
            .join("../../generated/models/eeg-demo/package.json");
        let document_text =
            fs::read_to_string(&package_path).expect("committed eeg-demo package.json should exist");

        let rendered = parse_and_render(&document_text, "eeg-demo/package.json")
            .expect("committed package.json should parse and render");
        assert!(rendered.contains("pub fn model() -> ::mdux::ModelPackage {"));
        assert!(rendered.contains("\"EEG-DOA-LINEAR\".to_string()"));
        assert!(rendered.contains("::mdux::MlLayer::Flatten"));
        assert!(rendered.contains("::mdux::MlLayer::Dense {"));
        assert!(rendered.contains("::mdux::MlLayer::Softmax"));
        assert!(rendered.contains("::mdux::MlTensor {"));

        let document: ModelPackageDocument =
            serde_json::from_str(&document_text).expect("committed package.json should parse");
        assert_eq!(
            rendered.matches("::mdux::MlGoldenVector {").count(),
            document.golden_vectors.len()
        );
    }

    #[test]
    fn rejects_a_non_f32_dtype_document() {
        let document_text = r#"{
            "model_id": "BAD",
            "dtype": "BF16",
            "input_spec": { "length": 1, "channels": 1 },
            "output_spec": { "classes": 1, "labels": ["A"] },
            "layers": [{ "kind": "flatten" }],
            "tensors": [],
            "golden_vectors": [],
            "evidence": {
                "package_sha256": "00",
                "toolchain_id": "rust-1.87.0",
                "build_recipe_sha256": "00",
                "weights_source_sha256": "00"
            }
        }"#;

        let error = parse_and_render(document_text, "bad-dtype.json")
            .expect_err("BF16 dtype must be rejected");
        assert!(error.to_string().contains("F32"));
    }

    #[test]
    fn rejects_malformed_json() {
        let error =
            parse_and_render("not json", "malformed.json").expect_err("malformed JSON must be rejected");
        assert!(error.to_string().contains("failed to parse"));
    }

    #[test]
    fn render_f32_vec_is_bit_exact_for_non_finite_values() {
        let values = [f32::NAN, f32::INFINITY, f32::NEG_INFINITY, 1.5f32];
        let rendered = render_f32_vec(&values);

        // `{:?}` would emit "NaN"/"inf"/"-inf", none of which are valid Rust float literals.
        assert!(!rendered.contains("NaN"));
        assert!(!rendered.contains("inf"));

        for value in values {
            assert!(rendered.contains(&format!("f32::from_bits({}u32)", value.to_bits())));
        }
    }
}
