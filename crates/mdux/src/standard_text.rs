use mdux_core::{MduxResult, Validates};
use mdux_text_schema::{
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

pub const DEFAULT_STANDARD_FONT: StandardFontDefinition = ROBOTO_REGULAR_400_16PX;

include!(concat!(
    env!("OUT_DIR"),
    "/default_standard_text_package.rs"
));

pub fn default_standard_text_package() -> MduxResult<TextPackage> {
    let package = build_default_standard_text_package();
    package.validate()?;
    Ok(package)
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
}
