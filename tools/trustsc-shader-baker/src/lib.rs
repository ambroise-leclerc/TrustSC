//! Host-only tool that compiles reviewed GLSL shader sources into committed SPIR-V evidence, per
//! ADR-007 (generated-artifact ownership) and ADR-012 (presentation adapter crates and shader
//! artifact evidence). `bake` produces `.spv` files plus a `report.json` recording per-artifact
//! SHA-256 digests and the exact shaderc options used; `verify` recompiles from the same reviewed
//! sources and fails loudly if the committed artifacts, or the toolchain that produced them,
//! have drifted.

use std::{
    fs,
    path::{Path, PathBuf},
};

use trustsc_core::{TrustScResult, ValidationError};
use serde::{Deserialize, Serialize};
use sha2::{Digest, Sha256};

const SCHEMA_VERSION: u32 = 1;
/// Pinned in `tools/trustsc-shader-baker/Cargo.toml`; recorded so `verify` can flag toolchain drift
/// even though the `shaderc` crate does not expose its own version at runtime.
const SHADERC_CRATE_VERSION: &str = "0.10.1";
const VULKAN_TARGET_ENV: &str = "vulkan1_0";
const SPIRV_TARGET_VERSION: &str = "1_0";
const OPTIMIZATION_LEVEL: &str = "performance";

#[derive(Debug, Deserialize)]
#[serde(deny_unknown_fields)]
struct Manifest {
    /// Directory containing shader sources and their `#include`s, relative to the manifest file.
    shader_dir: String,
    shaders: Vec<ShaderEntry>,
}

#[derive(Debug, Deserialize)]
#[serde(deny_unknown_fields)]
struct ShaderEntry {
    /// Source file name, relative to `shader_dir`.
    source: String,
    kind: ShaderKindSpec,
    /// Output file name, relative to the output directory.
    output: String,
}

#[derive(Debug, Deserialize, Clone, Copy)]
#[serde(rename_all = "lowercase")]
enum ShaderKindSpec {
    Vertex,
    Fragment,
}

impl ShaderKindSpec {
    fn to_shaderc(self) -> shaderc::ShaderKind {
        match self {
            Self::Vertex => shaderc::ShaderKind::Vertex,
            Self::Fragment => shaderc::ShaderKind::Fragment,
        }
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
struct ArtifactRecord {
    source: String,
    output: String,
    sha256: String,
}

#[derive(Debug, Serialize, Deserialize)]
struct Report {
    schema_version: u32,
    shaderc_crate_version: String,
    vulkan_target_env: String,
    spirv_target_version: String,
    optimization: String,
    warnings_as_errors: bool,
    artifacts: Vec<ArtifactRecord>,
}

#[derive(Clone, Copy, Debug)]
pub struct CliInvocation<'a> {
    pub manifest_path: &'a Path,
    pub output_dir: &'a Path,
    pub report_path: &'a Path,
}

pub struct BakeSummary {
    pub artifact_count: usize,
}

pub struct VerifySummary {
    pub artifact_count: usize,
}

pub fn bake(invocation: CliInvocation<'_>) -> TrustScResult<BakeSummary> {
    let (manifest, manifest_dir) = load_manifest(invocation.manifest_path)?;
    let shader_dir = manifest_dir.join(&manifest.shader_dir);

    fs::create_dir_all(invocation.output_dir).map_err(|error| {
        ValidationError::new(format!(
            "failed to create output directory {}: {error}",
            invocation.output_dir.display()
        ))
    })?;

    let mut artifacts = Vec::with_capacity(manifest.shaders.len());
    for shader in &manifest.shaders {
        let spirv = compile_shader(&shader_dir, shader)?;
        let output_path = invocation.output_dir.join(&shader.output);
        if let Some(parent) = output_path.parent() {
            fs::create_dir_all(parent).map_err(|error| {
                ValidationError::new(format!(
                    "failed to create output directory {}: {error}",
                    parent.display()
                ))
            })?;
        }
        fs::write(&output_path, &spirv).map_err(|error| {
            ValidationError::new(format!(
                "failed to write compiled shader {}: {error}",
                output_path.display()
            ))
        })?;

        artifacts.push(ArtifactRecord {
            source: shader.source.clone(),
            output: shader.output.clone(),
            sha256: hex_sha256(&spirv),
        });
    }

    let report = Report {
        schema_version: SCHEMA_VERSION,
        shaderc_crate_version: SHADERC_CRATE_VERSION.to_string(),
        vulkan_target_env: VULKAN_TARGET_ENV.to_string(),
        spirv_target_version: SPIRV_TARGET_VERSION.to_string(),
        optimization: OPTIMIZATION_LEVEL.to_string(),
        warnings_as_errors: true,
        artifacts,
    };
    write_report(invocation.report_path, &report)?;

    Ok(BakeSummary {
        artifact_count: report.artifacts.len(),
    })
}

pub fn verify(invocation: CliInvocation<'_>) -> TrustScResult<VerifySummary> {
    let (manifest, manifest_dir) = load_manifest(invocation.manifest_path)?;
    let shader_dir = manifest_dir.join(&manifest.shader_dir);

    let report_bytes = fs::read(invocation.report_path).map_err(|error| {
        ValidationError::new(format!(
            "failed to read report {}: {error}",
            invocation.report_path.display()
        ))
    })?;
    let report: Report = serde_json::from_slice(&report_bytes).map_err(|error| {
        ValidationError::new(format!(
            "failed to parse report {}: {error}",
            invocation.report_path.display()
        ))
    })?;

    if report.schema_version != SCHEMA_VERSION {
        return Err(ValidationError::new(format!(
            "report schema_version is {}, but this toolchain expects {}; rerun bake",
            report.schema_version, SCHEMA_VERSION
        )));
    }

    if report.shaderc_crate_version != SHADERC_CRATE_VERSION {
        return Err(ValidationError::new(format!(
            "report was baked with shaderc {}, but this toolchain pins {}; rerun bake",
            report.shaderc_crate_version, SHADERC_CRATE_VERSION
        )));
    }

    if report.vulkan_target_env != VULKAN_TARGET_ENV {
        return Err(ValidationError::new(format!(
            "report vulkan_target_env is {}, but this toolchain pins {}; rerun bake",
            report.vulkan_target_env, VULKAN_TARGET_ENV
        )));
    }

    if report.spirv_target_version != SPIRV_TARGET_VERSION {
        return Err(ValidationError::new(format!(
            "report spirv_target_version is {}, but this toolchain pins {}; rerun bake",
            report.spirv_target_version, SPIRV_TARGET_VERSION
        )));
    }

    if report.optimization != OPTIMIZATION_LEVEL {
        return Err(ValidationError::new(format!(
            "report optimization is {}, but this toolchain pins {}; rerun bake",
            report.optimization, OPTIMIZATION_LEVEL
        )));
    }

    if !report.warnings_as_errors {
        return Err(ValidationError::new(
            "report warnings_as_errors is false, but this toolchain requires true; rerun bake",
        ));
    }

    if report.artifacts.len() != manifest.shaders.len() {
        return Err(ValidationError::new(format!(
            "report lists {} artifacts but the manifest declares {}; rerun bake",
            report.artifacts.len(),
            manifest.shaders.len()
        )));
    }

    for (shader, recorded) in manifest.shaders.iter().zip(report.artifacts.iter()) {
        if shader.source != recorded.source || shader.output != recorded.output {
            return Err(ValidationError::new(format!(
                "manifest entry for {} does not match the recorded artifact {}; rerun bake",
                shader.source, recorded.source
            )));
        }

        let freshly_compiled = compile_shader(&shader_dir, shader)?;
        let fresh_sha256 = hex_sha256(&freshly_compiled);
        if fresh_sha256 != recorded.sha256 {
            return Err(ValidationError::new(format!(
                "{} recompiled to a different SPIR-V digest (recorded {}, now {}); the shader \
                 source or shaderc toolchain has drifted, rerun bake",
                shader.source, recorded.sha256, fresh_sha256
            )));
        }

        let committed_path = invocation.output_dir.join(&shader.output);
        let committed_bytes = fs::read(&committed_path).map_err(|error| {
            ValidationError::new(format!(
                "failed to read committed artifact {}: {error}",
                committed_path.display()
            ))
        })?;
        if committed_bytes != freshly_compiled {
            return Err(ValidationError::new(format!(
                "committed artifact {} does not match a fresh bake byte-for-byte; rerun bake",
                committed_path.display()
            )));
        }
    }

    Ok(VerifySummary {
        artifact_count: report.artifacts.len(),
    })
}

fn load_manifest(manifest_path: &Path) -> TrustScResult<(Manifest, PathBuf)> {
    let manifest_text = fs::read_to_string(manifest_path).map_err(|error| {
        ValidationError::new(format!(
            "failed to read manifest {}: {error}",
            manifest_path.display()
        ))
    })?;
    let manifest: Manifest = toml::from_str(&manifest_text).map_err(|error| {
        ValidationError::new(format!(
            "failed to parse manifest {}: {error}",
            manifest_path.display()
        ))
    })?;

    // `shader_dir` is deliberately allowed to contain `..` (it commonly walks up from
    // `tools/trustsc-shader-baker/fixtures/` to a sibling like `adapters/*/shaders/`), but it must
    // still be relative to the manifest file: `manifest_dir.join(absolute)` silently discards
    // `manifest_dir`, so an absolute value would let the manifest point anywhere on disk.
    if Path::new(&manifest.shader_dir).is_absolute() {
        return Err(ValidationError::new(format!(
            "manifest shader_dir {:?} must be relative to the manifest file, not absolute",
            manifest.shader_dir
        )));
    }
    for shader in &manifest.shaders {
        ensure_relative_within_scope(&shader.source, "shaders[].source")?;
        ensure_relative_within_scope(&shader.output, "shaders[].output")?;
    }

    let manifest_dir = manifest_path
        .parent()
        .map(Path::to_path_buf)
        .unwrap_or_default();

    Ok((manifest, manifest_dir))
}

/// Rejects absolute paths and `..` components so `source`/`output` manifest entries can only
/// name files inside the directory the caller already scoped them to (`shader_dir` for `source`,
/// the output directory for `output`), never escape it via a crafted or typo'd manifest entry.
fn ensure_relative_within_scope(value: &str, field: &str) -> TrustScResult<()> {
    let path = Path::new(value);
    if path.is_absolute()
        || path
            .components()
            .any(|component| matches!(component, std::path::Component::ParentDir))
    {
        return Err(ValidationError::new(format!(
            "manifest {field} {value:?} must be a relative path with no '..' components"
        )));
    }

    Ok(())
}

fn compile_shader(shader_dir: &Path, shader: &ShaderEntry) -> TrustScResult<Vec<u8>> {
    let source_path = shader_dir.join(&shader.source);
    let source_text = fs::read_to_string(&source_path).map_err(|error| {
        ValidationError::new(format!(
            "failed to read shader source {}: {error}",
            source_path.display()
        ))
    })?;

    let compiler = shaderc::Compiler::new().map_err(|error| {
        ValidationError::new(format!("failed to initialize the shaderc compiler: {error}"))
    })?;
    let mut options = shaderc::CompileOptions::new().map_err(|error| {
        ValidationError::new(format!(
            "failed to initialize shaderc compile options: {error}"
        ))
    })?;
    options.set_source_language(shaderc::SourceLanguage::GLSL);
    options.set_target_env(
        shaderc::TargetEnv::Vulkan,
        shaderc::EnvVersion::Vulkan1_0 as u32,
    );
    options.set_target_spirv(shaderc::SpirvVersion::V1_0);
    options.set_optimization_level(shaderc::OptimizationLevel::Performance);
    options.set_warnings_as_errors();

    let shader_dir_for_include = shader_dir.to_path_buf();
    options.set_include_callback(move |requested, _, source, _| {
        let requested_path = Path::new(requested);
        if requested_path.is_absolute()
            || requested_path
                .components()
                .any(|component| matches!(component, std::path::Component::ParentDir))
        {
            return Err(format!(
                "shader include '{requested}' from '{source}' must be a relative path within \
                 the shader directory (absolute paths and '..' are not allowed)"
            ));
        }

        let include_path = shader_dir_for_include.join(requested_path);
        let content = fs::read_to_string(&include_path).map_err(|error| {
            format!("failed to resolve shader include '{requested}' from '{source}': {error}")
        })?;

        Ok(shaderc::ResolvedInclude {
            resolved_name: include_path.to_string_lossy().into_owned(),
            content,
        })
    });

    let artifact = compiler
        .compile_into_spirv(
            &source_text,
            shader.kind.to_shaderc(),
            source_path.to_string_lossy().as_ref(),
            "main",
            Some(&options),
        )
        .map_err(|error| {
            ValidationError::new(format!(
                "failed to compile shader {}: {error}",
                source_path.display()
            ))
        })?;

    Ok(artifact.as_binary_u8().to_vec())
}

fn write_report(report_path: &Path, report: &Report) -> TrustScResult<()> {
    if let Some(parent) = report_path.parent() {
        fs::create_dir_all(parent).map_err(|error| {
            ValidationError::new(format!(
                "failed to create report directory {}: {error}",
                parent.display()
            ))
        })?;
    }

    let json = serde_json::to_string_pretty(report)
        .map_err(|error| ValidationError::new(format!("failed to serialize report: {error}")))?;
    fs::write(report_path, format!("{json}\n")).map_err(|error| {
        ValidationError::new(format!(
            "failed to write report {}: {error}",
            report_path.display()
        ))
    })
}

fn hex_sha256(bytes: &[u8]) -> String {
    let digest = Sha256::digest(bytes);
    digest.iter().map(|byte| format!("{byte:02x}")).collect()
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::time::{SystemTime, UNIX_EPOCH};

    fn temp_dir(prefix: &str) -> PathBuf {
        let unique = format!(
            "{}-{}-{}",
            prefix,
            std::process::id(),
            SystemTime::now()
                .duration_since(UNIX_EPOCH)
                .expect("system time should be after unix epoch")
                .as_nanos()
        );
        let path = std::env::temp_dir().join(unique);
        fs::create_dir_all(&path).expect("temporary directory should be creatable");
        path
    }

    fn fixture_manifest_path() -> PathBuf {
        PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("fixtures/text-shaders.toml")
    }

    #[test]
    fn bakes_and_verifies_fixture_manifest() {
        let manifest_path = fixture_manifest_path();
        let output_dir = temp_dir("trustsc-shader-baker-bake");
        let report_path = output_dir.join("report.json");
        let invocation = CliInvocation {
            manifest_path: &manifest_path,
            output_dir: &output_dir,
            report_path: &report_path,
        };

        let bake_summary = bake(invocation).expect("fixture manifest should bake successfully");
        assert_eq!(bake_summary.artifact_count, 8); // text + heightfield + flat + image pairs
        assert!(report_path.exists());

        let verify_summary =
            verify(invocation).expect("freshly baked artifacts should verify successfully");
        assert_eq!(verify_summary.artifact_count, bake_summary.artifact_count);

        fs::remove_dir_all(output_dir).expect("temporary output directory should be removable");
    }

    #[test]
    fn load_manifest_accepts_shader_dir_with_parent_dir_segments() {
        // shader_dir legitimately walks up from a fixtures/ directory to a sibling adapter
        // directory (see fixtures/text-shaders.toml); only an absolute shader_dir is rejected.
        let (manifest, _) =
            load_manifest(&fixture_manifest_path()).expect("fixture manifest should load");
        assert!(manifest.shader_dir.contains(".."));
    }

    #[test]
    fn load_manifest_rejects_absolute_shader_dir() {
        let root = temp_dir("trustsc-shader-baker-abs-shader-dir");
        let manifest_path = root.join("manifest.toml");
        fs::write(
            &manifest_path,
            "shader_dir = \"/etc\"\nshaders = []\n",
        )
        .expect("manifest should be writable");

        let error =
            load_manifest(&manifest_path).expect_err("absolute shader_dir should be rejected");
        assert!(error.to_string().contains("shader_dir"));

        fs::remove_dir_all(root).expect("temporary directory should be removable");
    }

    #[test]
    fn load_manifest_rejects_parent_dir_in_source() {
        let root = temp_dir("trustsc-shader-baker-dotdot-source");
        let manifest_path = root.join("manifest.toml");
        fs::write(
            &manifest_path,
            "shader_dir = \".\"\n\n[[shaders]]\nsource = \"../Cargo.toml\"\nkind = \"vertex\"\noutput = \"out.spv\"\n",
        )
        .expect("manifest should be writable");

        let error = load_manifest(&manifest_path)
            .expect_err("'..' in shaders[].source should be rejected");
        assert!(error.to_string().contains("source"));

        fs::remove_dir_all(root).expect("temporary directory should be removable");
    }

    #[test]
    fn load_manifest_rejects_absolute_output() {
        let root = temp_dir("trustsc-shader-baker-abs-output");
        let manifest_path = root.join("manifest.toml");
        fs::write(
            &manifest_path,
            "shader_dir = \".\"\n\n[[shaders]]\nsource = \"x.vert\"\nkind = \"vertex\"\noutput = \"/tmp/x.spv\"\n",
        )
        .expect("manifest should be writable");

        let error =
            load_manifest(&manifest_path).expect_err("absolute output should be rejected");
        assert!(error.to_string().contains("output"));

        fs::remove_dir_all(root).expect("temporary directory should be removable");
    }

    #[test]
    fn load_manifest_rejects_unknown_fields() {
        let root = temp_dir("trustsc-shader-baker-unknown-field");
        let manifest_path = root.join("manifest.toml");
        fs::write(
            &manifest_path,
            "shader_dir = \".\"\nshader_dirr = \"typo\"\nshaders = []\n",
        )
        .expect("manifest should be writable");

        let error =
            load_manifest(&manifest_path).expect_err("unknown manifest field should be rejected");
        assert!(error.to_string().contains("shader_dirr") || error.to_string().contains("unknown"));

        fs::remove_dir_all(root).expect("temporary directory should be removable");
    }
}
