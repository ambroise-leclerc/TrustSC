//! Approved image packages embedded from the committed `generated/images/` evidence (ADR-014),
//! mirroring `standard_text.rs` for fonts. The build script bakes every committed
//! `package.json` into `build_default_image_packages()`; this module validates on load.

use mdux_core::{MduxResult, Validates};
use mdux_image_schema::{ImageEvidence, ImagePackage};

/// Image id of the Acme placeholder logo (referenced from `.medui` via `img("LOGO-ACME")`).
pub const ACME_LOGO_IMAGE_ID: &str = "LOGO-ACME";

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct StandardImageDefinition {
    pub image_id: &'static str,
    pub width: u32,
    pub height: u32,
    pub package_json_path: &'static str,
}

pub const ACME_LOGO: StandardImageDefinition = StandardImageDefinition {
    image_id: ACME_LOGO_IMAGE_ID,
    width: 144,
    height: 48,
    package_json_path: "generated/images/acme-logo/package.json",
};

include!(concat!(env!("OUT_DIR"), "/default_image_packages.rs"));

/// All approved image packages, validated. Screens without `Image` nodes never need this.
pub fn default_image_packages() -> MduxResult<Vec<ImagePackage>> {
    let packages = build_default_image_packages();
    for package in &packages {
        package.validate()?;
    }
    reject_duplicate_ids(&packages)?;
    Ok(packages)
}

/// A duplicate id would make `img("...")` resolution ambiguous downstream (the compiler takes
/// the first match) — reject it here instead of letting an approved set compile silently.
fn reject_duplicate_ids(packages: &[ImagePackage]) -> MduxResult<()> {
    let mut seen_ids = std::collections::BTreeSet::new();
    for package in packages {
        if !seen_ids.insert(package.id.as_str()) {
            return Err(mdux_core::ValidationError::new(format!(
                "duplicate image package id {} in the approved image set",
                package.id
            )));
        }
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    fn stub_package(id: &str) -> ImagePackage {
        ImagePackage {
            id: id.to_string(),
            width: 1,
            height: 1,
            pixels: vec![0u8; 4],
            evidence: ImageEvidence {
                package_sha256: "0".repeat(64),
                source_sha256: "1".repeat(64),
                toolchain_id: "test".to_string(),
                build_recipe_sha256: "2".repeat(64),
            },
        }
    }

    #[test]
    fn rejects_duplicate_image_package_ids() {
        let unique = [stub_package("A"), stub_package("B")];
        reject_duplicate_ids(&unique).expect("unique ids should pass");

        let duplicated = [stub_package("A"), stub_package("B"), stub_package("A")];
        let error =
            reject_duplicate_ids(&duplicated).expect_err("duplicate id should be rejected");
        assert!(error.to_string().contains("duplicate image package id A"), "{error}");
    }

    #[test]
    fn loads_the_acme_logo_package() {
        let packages = default_image_packages().expect("image packages should load");
        let logo = packages
            .iter()
            .find(|package| package.id == ACME_LOGO.image_id)
            .expect("ACME logo package should exist");

        assert_eq!(logo.width, ACME_LOGO.width);
        assert_eq!(logo.height, ACME_LOGO.height);
        assert_eq!(
            logo.pixels.len(),
            (ACME_LOGO.width * ACME_LOGO.height * 4) as usize
        );
        // Straight-alpha RGBA: the generated placeholder is fully opaque.
        assert!(logo.pixels.chunks_exact(4).all(|pixel| pixel[3] == 0xFF));
    }
}
