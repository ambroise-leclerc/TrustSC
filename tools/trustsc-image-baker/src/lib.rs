//! Host-only image intake and deterministic package baking (ADR-014), mirroring
//! `trustsc-font-baker`'s bake/verify evidence contract: a TOML recipe points at a governed
//! image-asset manifest, `bake` produces byte-reproducible `package.json` + `report.json`
//! artifacts committed under `generated/images/`, and `verify` re-bakes and byte-compares.
//!
//! The vendored source format is binary PPM (P6) — parsed by the ~30 lines below, so no
//! image-decoding dependency enters the ADR-005 budget. The first governed asset (the Acme
//! placeholder logo) is produced by [`generate_placeholder_logo`], a pure-integer generator,
//! making the vendored file self-verifying by regeneration.

use std::fs;
use std::path::{Path, PathBuf};

use trustsc_core::{MduxResult, Validates, ValidationError, validate_non_empty};
use trustsc_image_schema::{ImageEvidence, ImagePackage};
use serde::{Deserialize, Serialize};
use sha2::{Digest, Sha256};

pub const TOOLCHAIN_ID: &str = "mdux-image-baker-0.1.0";

/// Intrinsic size of the generated Acme placeholder logo.
pub const PLACEHOLDER_LOGO_WIDTH: u32 = 144;
pub const PLACEHOLDER_LOGO_HEIGHT: u32 = 48;

#[derive(Clone, Debug, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct BakeRecipe {
    pub toolchain_id: String,
    /// Package id referenced from `.medui` files via `img("...")`.
    pub image_id: String,
    /// Path to the governed `image-manifest.toml`, relative to the workspace root.
    pub manifest: String,
}

#[derive(Clone, Debug, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct ImageManifest {
    pub schema_version: u32,
    pub manifest_kind: String,
    pub asset_id: String,
    pub license: String,
    pub asset: AssetSection,
    pub provenance: ProvenanceSection,
}

#[derive(Clone, Debug, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct AssetSection {
    pub source_file: String,
    pub source_format: String,
    pub source_sha256: String,
    pub width: u32,
    pub height: u32,
    pub intended_usage: String,
}

#[derive(Clone, Debug, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct ProvenanceSection {
    pub origin: String,
    pub generator: String,
}

pub struct CliInvocation<'a> {
    pub recipe_path: &'a Path,
    pub package_output_path: &'a Path,
    pub report_output_path: &'a Path,
}

#[derive(Debug)]
pub struct BakeSummary {
    pub package_sha256: String,
    pub source_sha256: String,
    pub width: u32,
    pub height: u32,
}

pub struct VerificationSummary {
    pub package_sha256: String,
    pub package_bytes_verified: usize,
    pub report_bytes_verified: usize,
}

#[derive(Debug)]
pub struct BakeArtifacts {
    pub package: ImagePackage,
    pub package_bytes: Vec<u8>,
    pub report_bytes: Vec<u8>,
    pub summary: BakeSummary,
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
            "failed to read baked image package {}: {error}",
            invocation.package_output_path.display()
        ))
    })?;
    let existing_report = fs::read(invocation.report_output_path).map_err(|error| {
        ValidationError::new(format!(
            "failed to read image bake report {}: {error}",
            invocation.report_output_path.display()
        ))
    })?;

    if existing_package != artifacts.package_bytes {
        return Err(ValidationError::new(
            "baked image package content does not match a fresh deterministic rebuild",
        ));
    }
    if existing_report != artifacts.report_bytes {
        return Err(ValidationError::new(
            "image bake report content does not match a fresh deterministic rebuild",
        ));
    }

    Ok(VerificationSummary {
        package_sha256: artifacts.summary.package_sha256,
        package_bytes_verified: existing_package.len(),
        report_bytes_verified: existing_report.len(),
    })
}

pub fn compile_recipe(recipe_path: impl AsRef<Path>) -> MduxResult<BakeArtifacts> {
    let recipe_path = canonical_existing_path(recipe_path.as_ref())?;
    let recipe_text = fs::read_to_string(&recipe_path).map_err(|error| {
        ValidationError::new(format!(
            "failed to read image bake recipe {}: {error}",
            recipe_path.display()
        ))
    })?;
    let recipe = toml::from_str::<BakeRecipe>(&recipe_text).map_err(|error| {
        ValidationError::new(format!(
            "failed to parse image bake recipe {}: {error}",
            recipe_path.display()
        ))
    })?;
    validate_non_empty("image recipe toolchain_id", &recipe.toolchain_id)?;
    validate_non_empty("image recipe image_id", &recipe.image_id)?;
    validate_non_empty("image recipe manifest", &recipe.manifest)?;

    let workspace_root = find_workspace_root(&recipe_path)?;
    let manifest_path = workspace_root.join(&recipe.manifest);
    let manifest_text = fs::read_to_string(&manifest_path).map_err(|error| {
        ValidationError::new(format!(
            "failed to read image manifest {}: {error}",
            manifest_path.display()
        ))
    })?;
    let manifest = toml::from_str::<ImageManifest>(&manifest_text).map_err(|error| {
        ValidationError::new(format!(
            "failed to parse image manifest {}: {error}",
            manifest_path.display()
        ))
    })?;
    if manifest.schema_version != 1 {
        return Err(ValidationError::new(
            "image manifest schema_version must be 1",
        ));
    }
    if manifest.manifest_kind != "mdux-image-asset" {
        return Err(ValidationError::new(format!(
            "manifest {} is not an mdux-image-asset manifest",
            manifest_path.display()
        )));
    }
    if manifest.asset.source_format != "ppm-p6" {
        return Err(ValidationError::new(format!(
            "unsupported image source format {} (only ppm-p6 is approved)",
            manifest.asset.source_format
        )));
    }

    let source_path = manifest_path
        .parent()
        .ok_or_else(|| ValidationError::new("image manifest has no parent directory"))?
        .join(&manifest.asset.source_file);
    let source_bytes = fs::read(&source_path).map_err(|error| {
        ValidationError::new(format!(
            "failed to read image source {}: {error}",
            source_path.display()
        ))
    })?;
    let source_sha256 = sha256_bytes(&source_bytes);
    if source_sha256 != manifest.asset.source_sha256 {
        return Err(ValidationError::new(format!(
            "image source {} sha256 {source_sha256} does not match the manifest's {}",
            source_path.display(),
            manifest.asset.source_sha256
        )));
    }

    let (width, height, rgb) = parse_ppm_p6(&source_bytes)?;
    if (width, height) != (manifest.asset.width, manifest.asset.height) {
        return Err(ValidationError::new(format!(
            "image source decodes to {width}x{height} but the manifest declares {}x{}",
            manifest.asset.width, manifest.asset.height
        )));
    }

    // RGB → straight-alpha RGBA (opaque).
    let mut pixels = Vec::with_capacity(rgb.len() / 3 * 4);
    for chunk in rgb.chunks_exact(3) {
        pixels.extend_from_slice(&[chunk[0], chunk[1], chunk[2], 0xFF]);
    }

    let recipe_sha256 = sha256_bytes(recipe_text.as_bytes());
    let mut package = ImagePackage {
        id: recipe.image_id.clone(),
        width,
        height,
        pixels,
        evidence: ImageEvidence {
            package_sha256: String::new(),
            source_sha256: source_sha256.clone(),
            toolchain_id: recipe.toolchain_id.clone(),
            build_recipe_sha256: recipe_sha256.clone(),
        },
    };
    // Canonical-hash pattern (same as the font baker): hash the package with an empty
    // package_sha256 field, then fill it in.
    let canonical_bytes = to_pretty_json(&ImagePackageDocument::from_package(&package))?;
    package.evidence.package_sha256 = sha256_bytes(&canonical_bytes);
    package.validate()?;

    let package_bytes = to_pretty_json(&ImagePackageDocument::from_package(&package))?;
    let report = ImageBakeReportDocument {
        report_kind: "mdux-image-baker-report".to_string(),
        package_sha256: package.evidence.package_sha256.clone(),
        recipe_sha256,
        source_sha256: source_sha256.clone(),
        image_id: recipe.image_id,
        width,
        height,
        manifest_path: recipe.manifest,
        pipeline: "image-intake -> ppm-decode -> rgba-package -> package-verify".to_string(),
    };
    let report_bytes = to_pretty_json(&report)?;

    Ok(BakeArtifacts {
        summary: BakeSummary {
            package_sha256: package.evidence.package_sha256.clone(),
            source_sha256,
            width,
            height,
        },
        package,
        package_bytes,
        report_bytes,
    })
}

/// Loads a baked `package.json` back into an [`ImagePackage`] (used by the facade embed and
/// tests).
pub fn parse_package_json(bytes: &[u8]) -> MduxResult<ImagePackage> {
    let document: ImagePackageDocument = serde_json::from_slice(bytes).map_err(|error| {
        ValidationError::new(format!("failed to parse image package JSON: {error}"))
    })?;
    let package = document.into_package()?;
    package.validate()?;
    Ok(package)
}

/// Deterministic two-tone Acme placeholder mark, 144×48 RGB — pure integer math so the
/// vendored PPM is bit-reproducible. Layout: light-gray field, a hollow triangular "A" on the
/// left, a dark wordmark block and a green accent bar on the right.
pub fn generate_placeholder_logo() -> Vec<u8> {
    const BG: [u8; 3] = [209, 214, 219]; // light gray, matches Theme.Colors.TopbarBackground
    const INK: [u8; 3] = [26, 31, 41]; // near-black slate
    const ACCENT: [u8; 3] = [33, 184, 107]; // green

    let width = PLACEHOLDER_LOGO_WIDTH as i32;
    let height = PLACEHOLDER_LOGO_HEIGHT as i32;
    let mut rgb = Vec::with_capacity((width * height * 3) as usize);

    for y in 0..height {
        for x in 0..width {
            // Outer triangle: apex (28, 8), base y = 40 from x 12 to 44.
            let in_outer = y >= 8 && y <= 40 && {
                let half_width = (y - 8) * 16 / 32;
                x >= 28 - half_width && x <= 28 + half_width
            };
            // Inner cut-out making the "A" hollow: apex (28, 22), base y = 38; the crossbar
            // rows 30..=33 stay filled.
            let in_inner = y >= 22 && y <= 38 && !(30..=33).contains(&y) && {
                let half_width = (y - 22) * 7 / 16;
                x >= 28 - half_width && x <= 28 + half_width
            };
            // Wordmark block and accent bar.
            let in_wordmark = (56..=132).contains(&x) && (12..=24).contains(&y);
            let in_accent = (56..=132).contains(&x) && (30..=36).contains(&y);

            let color = if in_outer && !in_inner {
                INK
            } else if in_wordmark {
                INK
            } else if in_accent {
                ACCENT
            } else {
                BG
            };
            rgb.extend_from_slice(&color);
        }
    }

    rgb
}

/// Serializes the placeholder logo as a binary PPM (P6) file.
pub fn placeholder_logo_ppm() -> Vec<u8> {
    let mut bytes = format!(
        "P6\n{PLACEHOLDER_LOGO_WIDTH} {PLACEHOLDER_LOGO_HEIGHT}\n255\n"
    )
    .into_bytes();
    bytes.extend_from_slice(&generate_placeholder_logo());
    bytes
}

/// Minimal binary-PPM (P6) parser: `P6`, whitespace/comment-separated width/height/maxval,
/// one whitespace byte, then `width * height * 3` RGB bytes.
pub fn parse_ppm_p6(bytes: &[u8]) -> MduxResult<(u32, u32, Vec<u8>)> {
    let mut cursor = 0usize;
    let mut next_token = |bytes: &[u8]| -> MduxResult<String> {
        // Skip whitespace and `#` comments.
        loop {
            while cursor < bytes.len() && bytes[cursor].is_ascii_whitespace() {
                cursor += 1;
            }
            if cursor < bytes.len() && bytes[cursor] == b'#' {
                while cursor < bytes.len() && bytes[cursor] != b'\n' {
                    cursor += 1;
                }
                continue;
            }
            break;
        }
        let start = cursor;
        while cursor < bytes.len() && !bytes[cursor].is_ascii_whitespace() {
            cursor += 1;
        }
        if start == cursor {
            return Err(ValidationError::new("truncated PPM header"));
        }
        Ok(String::from_utf8_lossy(&bytes[start..cursor]).into_owned())
    };

    if next_token(bytes)? != "P6" {
        return Err(ValidationError::new("image source is not a binary PPM (P6)"));
    }
    let width: u32 = next_token(bytes)?
        .parse()
        .map_err(|_| ValidationError::new("invalid PPM width"))?;
    let height: u32 = next_token(bytes)?
        .parse()
        .map_err(|_| ValidationError::new("invalid PPM height"))?;
    let maxval: u32 = next_token(bytes)?
        .parse()
        .map_err(|_| ValidationError::new("invalid PPM maxval"))?;
    if maxval != 255 {
        return Err(ValidationError::new("PPM maxval must be 255"));
    }
    // Per the PPM spec, exactly one whitespace byte terminates the header before the binary
    // pixel data begins — verify it's actually whitespace rather than blindly skipping a byte,
    // which could otherwise misalign the decode on a malformed or non-conforming file.
    match bytes.get(cursor) {
        Some(byte) if byte.is_ascii_whitespace() => cursor += 1,
        _ => return Err(ValidationError::new("PPM header is missing its whitespace terminator")),
    }

    let expected = (width as usize)
        .checked_mul(height as usize)
        .and_then(|pixels| pixels.checked_mul(3))
        .ok_or_else(|| ValidationError::new("PPM dimensions overflow usize"))?;
    let data_end = cursor
        .checked_add(expected)
        .ok_or_else(|| ValidationError::new("PPM dimensions overflow usize"))?;
    let data = bytes
        .get(cursor..data_end)
        .ok_or_else(|| ValidationError::new("PPM pixel data is truncated"))?;
    if data_end != bytes.len() {
        return Err(ValidationError::new("PPM has trailing bytes after pixel data"));
    }
    Ok((width, height, data.to_vec()))
}

#[derive(Deserialize, Serialize)]
struct ImagePackageDocument {
    id: String,
    width: u32,
    height: u32,
    pixels_hex: String,
    evidence: ImageEvidenceDocument,
}

#[derive(Deserialize, Serialize)]
struct ImageEvidenceDocument {
    package_sha256: String,
    source_sha256: String,
    toolchain_id: String,
    build_recipe_sha256: String,
}

impl ImagePackageDocument {
    fn from_package(package: &ImagePackage) -> Self {
        Self {
            id: package.id.clone(),
            width: package.width,
            height: package.height,
            pixels_hex: hex_encode(&package.pixels),
            evidence: ImageEvidenceDocument {
                package_sha256: package.evidence.package_sha256.clone(),
                source_sha256: package.evidence.source_sha256.clone(),
                toolchain_id: package.evidence.toolchain_id.clone(),
                build_recipe_sha256: package.evidence.build_recipe_sha256.clone(),
            },
        }
    }

    fn into_package(self) -> MduxResult<ImagePackage> {
        Ok(ImagePackage {
            id: self.id,
            width: self.width,
            height: self.height,
            pixels: hex_decode(&self.pixels_hex)?,
            evidence: ImageEvidence {
                package_sha256: self.evidence.package_sha256,
                source_sha256: self.evidence.source_sha256,
                toolchain_id: self.evidence.toolchain_id,
                build_recipe_sha256: self.evidence.build_recipe_sha256,
            },
        })
    }
}

#[derive(Serialize)]
struct ImageBakeReportDocument {
    report_kind: String,
    package_sha256: String,
    recipe_sha256: String,
    source_sha256: String,
    image_id: String,
    width: u32,
    height: u32,
    manifest_path: String,
    pipeline: String,
}

fn hex_encode(bytes: &[u8]) -> String {
    let mut encoded = String::with_capacity(bytes.len() * 2);
    for byte in bytes {
        encoded.push_str(&format!("{byte:02x}"));
    }
    encoded
}

fn hex_decode(text: &str) -> MduxResult<Vec<u8>> {
    if text.len() % 2 != 0 {
        return Err(ValidationError::new("pixels_hex must have even length"));
    }
    (0..text.len())
        .step_by(2)
        .map(|index| {
            u8::from_str_radix(&text[index..index + 2], 16)
                .map_err(|_| ValidationError::new("pixels_hex contains non-hex characters"))
        })
        .collect()
}

fn sha256_bytes(bytes: &[u8]) -> String {
    let mut hasher = Sha256::new();
    hasher.update(bytes);
    hex_encode(&hasher.finalize())
}

fn to_pretty_json<T: Serialize>(value: &T) -> MduxResult<Vec<u8>> {
    let mut bytes = serde_json::to_vec_pretty(value)
        .map_err(|error| ValidationError::new(format!("failed to serialize JSON: {error}")))?;
    bytes.push(b'\n');
    Ok(bytes)
}

fn write_bytes(path: &Path, bytes: &[u8]) -> MduxResult<()> {
    // `path.parent()` returns `Some("")` (not `None`) for a bare filename like "package.json";
    // `create_dir_all("")` would then fail even though there's no directory to create.
    let parent = path.parent().filter(|parent| !parent.as_os_str().is_empty());
    if let Some(parent) = parent {
        fs::create_dir_all(parent).map_err(|error| {
            ValidationError::new(format!(
                "failed to create output directory {}: {error}",
                parent.display()
            ))
        })?;
    }
    fs::write(path, bytes).map_err(|error| {
        ValidationError::new(format!("failed to write {}: {error}", path.display()))
    })
}

fn canonical_existing_path(path: &Path) -> MduxResult<PathBuf> {
    fs::canonicalize(path).map_err(|error| {
        ValidationError::new(format!(
            "failed to canonicalize path {}: {error}",
            path.display()
        ))
    })
}

/// Walks up from the recipe to the workspace root (the directory whose Cargo.toml declares
/// `[workspace]`).
fn find_workspace_root(start: &Path) -> MduxResult<PathBuf> {
    let mut current = start.parent();
    while let Some(directory) = current {
        let manifest = directory.join("Cargo.toml");
        if manifest.exists() {
            let text = fs::read_to_string(&manifest).map_err(|error| {
                ValidationError::new(format!(
                    "failed to read {}: {error}",
                    manifest.display()
                ))
            })?;
            if text.contains("[workspace]") {
                return Ok(directory.to_path_buf());
            }
        }
        current = directory.parent();
    }
    Err(ValidationError::new(
        "could not locate the workspace root above the recipe",
    ))
}

#[cfg(test)]
mod tests {
    use super::*;

    fn workspace_root() -> PathBuf {
        PathBuf::from(env!("CARGO_MANIFEST_DIR"))
            .parent()
            .and_then(Path::parent)
            .expect("workspace root should exist")
            .to_path_buf()
    }

    #[test]
    fn vendored_placeholder_logo_is_bit_reproducible() {
        // The governed asset is self-verifying: regenerating the deterministic placeholder
        // must produce exactly the vendored bytes.
        let vendored = fs::read(
            workspace_root().join("assets/images/acme-logo/logo-acme-144x48.ppm"),
        )
        .expect("vendored placeholder logo should exist");
        assert_eq!(
            sha256_bytes(&vendored),
            sha256_bytes(&placeholder_logo_ppm()),
            "vendored PPM must equal the deterministic generator's output"
        );
    }

    #[test]
    fn bakes_and_verifies_the_acme_logo_fixture() {
        let recipe = workspace_root().join("tools/trustsc-image-baker/fixtures/acme-logo.toml");
        let artifacts = compile_recipe(&recipe).expect("fixture should bake");
        assert_eq!(artifacts.package.id, "LOGO-ACME");
        assert_eq!(artifacts.package.width, PLACEHOLDER_LOGO_WIDTH);
        assert_eq!(artifacts.package.height, PLACEHOLDER_LOGO_HEIGHT);
        artifacts.package.validate().expect("baked package validates");

        // Round-trip: the emitted package.json parses back into an identical package.
        let round_tripped =
            parse_package_json(&artifacts.package_bytes).expect("package.json parses back");
        assert_eq!(round_tripped, artifacts.package);

        // The committed evidence matches a fresh rebuild byte for byte.
        let generated = workspace_root().join("generated/images/acme-logo");
        let invocation = CliInvocation {
            recipe_path: &recipe,
            package_output_path: &generated.join("package.json"),
            report_output_path: &generated.join("report.json"),
        };
        let summary = verify(invocation).expect("committed evidence should verify");
        assert_eq!(summary.package_sha256, artifacts.summary.package_sha256);
    }

    #[test]
    fn rejects_malformed_ppm_sources() {
        assert!(parse_ppm_p6(b"P3\n2 2\n255\n").is_err(), "ASCII PPM rejected");
        assert!(parse_ppm_p6(b"P6\n2 2\n255\n\x00\x00").is_err(), "truncated rejected");
        let mut trailing = placeholder_logo_ppm();
        trailing.push(0);
        assert!(parse_ppm_p6(&trailing).is_err(), "trailing bytes rejected");
        // The header must be terminated by an actual whitespace byte, not a blindly-skipped one.
        assert!(
            parse_ppm_p6(b"P6\n2 2\n255X\x00\x00\x00\x00\x00\x00\x00\x00\x00\x00\x00\x00")
                .is_err(),
            "missing whitespace terminator rejected"
        );
        // Dimensions that would overflow width*height*3 in usize arithmetic must error, not
        // panic or wrap.
        assert!(
            parse_ppm_p6(format!("P6\n{} {}\n255\n", u32::MAX, u32::MAX).as_bytes()).is_err(),
            "overflowing dimensions rejected"
        );
        // The real asset parses.
        let (width, height, rgb) = parse_ppm_p6(&placeholder_logo_ppm()).expect("valid P6");
        assert_eq!((width, height), (144, 48));
        assert_eq!(rgb.len(), 144 * 48 * 3);
    }

    #[test]
    fn rejects_an_unsupported_manifest_schema_version() {
        // find_workspace_root walks up looking for a Cargo.toml with `[workspace]`, so the
        // scratch recipe must live inside the repo tree (target/ is gitignored and safe).
        let mut recipe_dir = workspace_root().join("target");
        recipe_dir.push(format!("trustsc-image-baker-schema-version-test-{}", std::process::id()));
        fs::create_dir_all(&recipe_dir).expect("temp dir");
        let manifest_path = recipe_dir.join("image-manifest.toml");
        fs::write(
            &manifest_path,
            r#"
schema_version = 2
manifest_kind = "mdux-image-asset"
asset_id = "test"
license = "CC0-1.0"

[asset]
source_file = "logo.ppm"
source_format = "ppm-p6"
source_sha256 = "0000000000000000000000000000000000000000000000000000000000000"
width = 1
height = 1
intended_usage = "test"

[provenance]
origin = "test"
generator = "test"
"#,
        )
        .expect("write manifest");
        let recipe_path = recipe_dir.join("recipe.toml");
        fs::write(
            &recipe_path,
            format!(
                "toolchain_id = \"test\"\nimage_id = \"TEST\"\nmanifest = \"{}\"\n",
                manifest_path.display()
            ),
        )
        .expect("write recipe");

        let error = compile_recipe(&recipe_path).expect_err("schema_version 2 must be rejected");
        assert!(error.to_string().contains("schema_version must be 1"), "{error}");

        let _ = fs::remove_dir_all(&recipe_dir);
    }

    #[test]
    fn write_bytes_accepts_a_bare_filename_with_no_directory_component() {
        let mut path = std::env::temp_dir();
        path.push(format!("trustsc-image-baker-write-bytes-test-{}", std::process::id()));
        fs::create_dir_all(&path).expect("temp dir");
        let original_dir = std::env::current_dir().expect("cwd");
        std::env::set_current_dir(&path).expect("chdir");

        let result = write_bytes(Path::new("bare-file.json"), b"{}");

        std::env::set_current_dir(original_dir).expect("restore cwd");
        let _ = fs::remove_dir_all(&path);

        result.expect("writing a bare filename with an empty parent should not fail");
    }
}
