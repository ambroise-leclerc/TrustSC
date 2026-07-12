use trustsc_core::{TrustScResult, Validates};
use trustsc_text_schema::{
    ApprovedString, AtlasGlyph, CompiledGlyph, CompiledTextRun, DeterminismEvidence, FontAsset,
    NumericGlyphEntry, NumericGlyphSet, NumericTemplate, TextDirection, TextPackage, TextureAtlas,
};

pub const DEFAULT_STANDARD_HELLO_WORLD_TEXT: &str = "Hello World!";
pub const DEFAULT_STANDARD_HELLO_WORLD_STRING_ID: &str = "STR-HELLO-WORLD";
pub const DEFAULT_STANDARD_HELLO_WORLD_RUN_ID: &str = "RUN-HELLO-WORLD";
pub const DEFAULT_STANDARD_FONT_SOURCE_PATH: &str = "assets/fonts/roboto/Roboto-Regular.ttf";

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct StandardFontDefinition {
    pub family: &'static str,
    pub weight: u16,
    pub pixel_height: u16,
    pub package_json_path: &'static str,
}

pub const ROBOTO_REGULAR_400_16PX: StandardFontDefinition = StandardFontDefinition {
    family: "Roboto",
    weight: 400,
    pixel_height: 16,
    package_json_path: "generated/fonts/roboto-regular-16px/package.json",
};

pub const ROBOTO_DISPLAY_400_48PX: StandardFontDefinition = StandardFontDefinition {
    family: "Roboto",
    weight: 400,
    pixel_height: 48,
    package_json_path: "generated/fonts/roboto-display-48px/package.json",
};

pub const ROBOTO_DISPLAY_400_160PX: StandardFontDefinition = StandardFontDefinition {
    family: "Roboto",
    weight: 400,
    pixel_height: 160,
    package_json_path: "generated/fonts/roboto-display-160px/package.json",
};

pub const DEFAULT_STANDARD_FONT: StandardFontDefinition = ROBOTO_REGULAR_400_16PX;
pub const DEFAULT_DISPLAY_FONT: StandardFontDefinition = ROBOTO_DISPLAY_400_48PX;

/// Digit glyph set id of the display package (48 px, digits only).
pub const DEFAULT_DISPLAY_DIGITS_GLYPH_SET_ID: &str = "SET-DISPLAY-DIGITS-48";
/// Digit glyph set id of the large display package (160 px = 120 pt, digits only, ADR-014).
pub const DISPLAY_160_DIGITS_GLYPH_SET_ID: &str = "SET-DISPLAY-DIGITS-160";
/// Digit + separator glyph set id of the standard package (16 px, `0-9`, `-`, `:`, space) —
/// the set the clock and date formatters render from.
pub const DEFAULT_STANDARD_DIGITS_GLYPH_SET_ID: &str = "SET-ASCII-DIGITS";
/// Printable-ASCII glyph set id of the standard package (16 px, space through `~`) — the baked
/// charset TextInput operator entry renders from and is validated against (ADR-015).
pub const STANDARD_ASCII_TEXT_GLYPH_SET_ID: &str = "SET-ASCII-TEXT";

include!(concat!(
    env!("OUT_DIR"),
    "/default_standard_text_package.rs"
));

include!(concat!(
    env!("OUT_DIR"),
    "/default_display_text_package.rs"
));

include!(concat!(
    env!("OUT_DIR"),
    "/default_display_160_text_package.rs"
));

pub fn default_standard_text_package() -> TrustScResult<TextPackage> {
    let package = build_default_standard_text_package();
    package.validate()?;
    Ok(package)
}

/// The approved display-size package (48 px digits) for big-numeral realtime displays, baked
/// from `tools/trustsc-font-baker/fixtures/roboto-display-48px.toml` (ADR-013 two-package strategy).
pub fn default_display_text_package() -> TrustScResult<TextPackage> {
    let package = build_default_display_text_package();
    package.validate()?;
    Ok(package)
}

/// The approved 160 px (= 120 pt) display package, baked from
/// `tools/trustsc-font-baker/fixtures/roboto-display-160px.toml` (ADR-014).
pub fn default_display_160_text_package() -> TrustScResult<TextPackage> {
    let package = build_default_display_160_text_package();
    package.validate()?;
    Ok(package)
}

/// Every approved display package, in ascending pixel-height order. NumericDisplay templates
/// are resolved across all of them with unique-match semantics (ADR-014).
pub fn default_display_text_packages() -> TrustScResult<Vec<TextPackage>> {
    Ok(vec![
        default_display_text_package()?,
        default_display_160_text_package()?,
    ])
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn loads_default_standard_roboto_package() {
        let package =
            default_standard_text_package().expect("default standard package should load");

        assert_eq!(package.fonts.len(), 1);
        assert_eq!(package.fonts[0].family, DEFAULT_STANDARD_FONT.family);
        assert_eq!(
            package.fonts[0].pixel_height,
            DEFAULT_STANDARD_FONT.pixel_height
        );
        assert_eq!(
            package.fonts[0].source_path,
            DEFAULT_STANDARD_FONT_SOURCE_PATH
        );
        assert_eq!(DEFAULT_STANDARD_FONT.weight, 400);

        let approved_string = package
            .approved_strings
            .iter()
            .find(|approved_string| approved_string.id == DEFAULT_STANDARD_HELLO_WORLD_STRING_ID)
            .expect("hello world string should exist");

        assert_eq!(approved_string.value, DEFAULT_STANDARD_HELLO_WORLD_TEXT);
        assert_eq!(
            package
                .find_run(DEFAULT_STANDARD_HELLO_WORLD_RUN_ID)
                .expect("hello world run should exist")
                .source_string_id,
            DEFAULT_STANDARD_HELLO_WORLD_STRING_ID
        );
        assert!(package.find_template("TPL-DOSE").is_some());
    }

    #[test]
    fn standard_package_carries_monitor_strings_and_clock_glyphs() {
        let package =
            default_standard_text_package().expect("default standard package should load");

        for (string_id, locales) in [
            ("STR-NS-TITLE", ["en-US", "fr-FR"]),
            ("STR-NS-NOMINAL", ["en-US", "fr-FR"]),
            ("STR-NS-ALERT", ["en-US", "fr-FR"]),
            ("STR-NS-FAULT", ["en-US", "fr-FR"]),
            ("STR-NS-ACK", ["en-US", "fr-FR"]),
            ("STR-NS-PATIENT-ID", ["en-US", "fr-FR"]),
        ] {
            for locale in locales {
                assert!(
                    package.find_run_for_string(string_id, locale).is_some(),
                    "missing compiled run for {string_id} in {locale}"
                );
            }
        }

        let digits = package
            .find_numeric_glyph_set(DEFAULT_STANDARD_DIGITS_GLYPH_SET_ID)
            .expect("standard digit glyph set should exist");
        for required in ['0', '9', '-', ':', ' '] {
            assert!(
                digits.entries.iter().any(|entry| entry.character == required),
                "standard digit set is missing '{required}'"
            );
        }
    }

    #[test]
    fn standard_package_carries_the_full_printable_ascii_text_set() {
        let package =
            default_standard_text_package().expect("default standard package should load");

        let ascii_text = package
            .find_numeric_glyph_set(STANDARD_ASCII_TEXT_GLYPH_SET_ID)
            .expect("printable ASCII text glyph set should exist");

        // Every printable ASCII character (space through '~'), each exactly once — the
        // complete charset TextInput operator entry is bounded to (ADR-015).
        assert_eq!(ascii_text.entries.len(), 95);
        for code in 0x20u8..=0x7E {
            let required = code as char;
            assert!(
                ascii_text.entries.iter().any(|entry| entry.character == required),
                "printable ASCII set is missing '{required}'"
            );
        }
    }

    #[test]
    fn loads_default_display_roboto_package() {
        let package = default_display_text_package().expect("display package should load");

        assert_eq!(package.fonts.len(), 1);
        assert_eq!(
            package.fonts[0].pixel_height,
            DEFAULT_DISPLAY_FONT.pixel_height
        );

        let template = package
            .find_template("TPL-SEDATION-INDEX")
            .expect("sedation index template should exist");
        assert_eq!(template.prefix_run_id, None);
        assert_eq!(template.suffix_run_id, None);
        assert_eq!(template.max_chars, 2);
        assert_eq!(template.glyph_set_id, DEFAULT_DISPLAY_DIGITS_GLYPH_SET_ID);

        let digits = package
            .find_numeric_glyph_set(DEFAULT_DISPLAY_DIGITS_GLYPH_SET_ID)
            .expect("display digit glyph set should exist");
        assert_eq!(digits.entries.len(), 10);
    }
}
