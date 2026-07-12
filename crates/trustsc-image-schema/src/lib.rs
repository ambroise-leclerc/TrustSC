//! Immutable compiled image-package schema (ADR-014) — the contract between the host-side
//! image baker (`tools/trustsc-image-baker`) and every governed consumer. Mirrors the role
//! `trustsc-text-schema` plays for fonts: authoring bakes deterministic evidence offline, the
//! runtime only ever consumes validated, immutable packages.

#![forbid(unsafe_code)]

use trustsc_core::{TrustScResult, Validates, ValidationError, validate_non_empty};

/// A governed raster image: straight-alpha RGBA8, row-major, top-left origin. Rendered at its
/// intrinsic size only — consumers verify declared bounds equal `width`×`height` exactly, so
/// there is no runtime scaling (the pixel-domain analogue of "no runtime shaping").
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct ImagePackage {
    pub id: String,
    pub width: u32,
    pub height: u32,
    /// RGBA8, `width * height * 4` bytes.
    pub pixels: Vec<u8>,
    pub evidence: ImageEvidence,
}

/// Determinism evidence carried by every baked image package (ADR-007 pattern).
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct ImageEvidence {
    pub package_sha256: String,
    pub source_sha256: String,
    pub toolchain_id: String,
    pub build_recipe_sha256: String,
}

impl Validates for ImageEvidence {
    fn validate(&self) -> TrustScResult<()> {
        validate_non_empty("image package_sha256", &self.package_sha256)?;
        validate_non_empty("image source_sha256", &self.source_sha256)?;
        validate_non_empty("image toolchain_id", &self.toolchain_id)?;
        validate_non_empty("image build_recipe_sha256", &self.build_recipe_sha256)?;

        if !is_sha256(&self.package_sha256)
            || !is_sha256(&self.source_sha256)
            || !is_sha256(&self.build_recipe_sha256)
        {
            return Err(ValidationError::new(
                "image evidence digests must be 64-character lowercase hexadecimal values",
            ));
        }
        Ok(())
    }
}

impl Validates for ImagePackage {
    fn validate(&self) -> TrustScResult<()> {
        validate_non_empty("image package id", &self.id)?;
        if self.width == 0 || self.height == 0 {
            return Err(ValidationError::new(
                "image dimensions must be strictly positive",
            ));
        }
        let expected = (self.width as usize)
            .checked_mul(self.height as usize)
            .and_then(|pixels| pixels.checked_mul(4))
            .ok_or_else(|| ValidationError::new("image dimensions overflow usize"))?;
        if self.pixels.len() != expected {
            return Err(ValidationError::new(format!(
                "image pixel buffer must be width * height * 4 bytes (expected {expected}, got {})",
                self.pixels.len()
            )));
        }
        self.evidence.validate()
    }
}

fn is_sha256(value: &str) -> bool {
    value.len() == 64
        && value
            .chars()
            .all(|character| character.is_ascii_hexdigit() && !character.is_ascii_uppercase())
}

#[cfg(test)]
mod tests {
    use super::*;

    fn sample() -> ImagePackage {
        ImagePackage {
            id: "LOGO-TEST".to_string(),
            width: 2,
            height: 2,
            pixels: vec![0u8; 16],
            evidence: ImageEvidence {
                package_sha256: "0".repeat(64),
                source_sha256: "1".repeat(64),
                toolchain_id: "rust-test".to_string(),
                build_recipe_sha256: "2".repeat(64),
            },
        }
    }

    #[test]
    fn validates_a_well_formed_package() {
        sample().validate().expect("sample should validate");
    }

    #[test]
    fn rejects_a_pixel_buffer_of_the_wrong_size() {
        let mut package = sample();
        package.pixels.pop();
        let error = package.validate().expect_err("short buffer rejected");
        assert!(error.to_string().contains("width * height * 4"));
    }

    #[test]
    fn rejects_zero_dimensions_and_malformed_digests() {
        let mut package = sample();
        package.width = 0;
        assert!(package.validate().is_err());

        let mut package = sample();
        package.evidence.source_sha256 = "not-a-digest".to_string();
        assert!(package.validate().is_err());
    }
}
