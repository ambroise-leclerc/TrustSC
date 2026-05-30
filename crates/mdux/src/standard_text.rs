use mdux_core::{MduxResult, ValidationError, Validates};
use mdux_text_schema::{
    ApprovedString, AtlasGlyph, CompiledGlyph, CompiledTextRun, DeterminismEvidence, FontAsset,
    NumericGlyphEntry, NumericGlyphSet, NumericTemplate, TextDirection, TextPackage, TextureAtlas,
};
use serde::Deserialize;

pub const DEFAULT_STANDARD_HELLO_WORLD_TEXT: &str = "Hello World!";
pub const DEFAULT_STANDARD_HELLO_WORLD_STRING_ID: &str = "STR-HELLO-WORLD";
pub const DEFAULT_STANDARD_HELLO_WORLD_RUN_ID: &str = "RUN-HELLO-WORLD";

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

const DEFAULT_STANDARD_FONT_PACKAGE_JSON: &str = include_str!(concat!(
    env!("CARGO_MANIFEST_DIR"),
    "/../../generated/fonts/roboto-regular-16px/package.json"
));

pub fn default_standard_text_package() -> MduxResult<TextPackage> {
    parse_text_package_document(
        DEFAULT_STANDARD_FONT.package_json_path,
        DEFAULT_STANDARD_FONT_PACKAGE_JSON,
    )
}

fn parse_text_package_document(package_path: &str, document: &str) -> MduxResult<TextPackage> {
    let package_document: PackageDocument = serde_json::from_str(document).map_err(|error| {
        ValidationError::new(format!(
            "failed to deserialize text package {package_path}: {error}"
        ))
    })?;
    let package = package_document.into_text_package(package_path)?;
    package.validate()?;
    Ok(package)
}

#[derive(Clone, Debug, Deserialize)]
#[serde(deny_unknown_fields)]
struct PackageDocument {
    fonts: Vec<FontAssetDocument>,
    approved_strings: Vec<ApprovedStringDocument>,
    atlases: Vec<TextureAtlasDocument>,
    atlas_glyphs: Vec<AtlasGlyphDocument>,
    runs: Vec<CompiledTextRunDocument>,
    numeric_glyph_sets: Vec<NumericGlyphSetDocument>,
    numeric_templates: Vec<NumericTemplateDocument>,
    evidence: DeterminismEvidenceDocument,
}

impl PackageDocument {
    fn into_text_package(self, package_path: &str) -> MduxResult<TextPackage> {
        Ok(TextPackage {
            fonts: self.fonts.into_iter().map(Into::into).collect(),
            approved_strings: self
                .approved_strings
                .into_iter()
                .map(ApprovedStringDocument::into_approved_string)
                .collect::<MduxResult<_>>()?,
            atlases: self
                .atlases
                .into_iter()
                .enumerate()
                .map(|(atlas_index, atlas)| atlas.into_texture_atlas(package_path, atlas_index))
                .collect::<MduxResult<_>>()?,
            atlas_glyphs: self.atlas_glyphs.into_iter().map(Into::into).collect(),
            runs: self.runs.into_iter().map(Into::into).collect(),
            numeric_glyph_sets: self.numeric_glyph_sets.into_iter().map(Into::into).collect(),
            numeric_templates: self.numeric_templates.into_iter().map(Into::into).collect(),
            evidence: self.evidence.into(),
        })
    }
}

#[derive(Clone, Debug, Deserialize)]
#[serde(deny_unknown_fields)]
struct FontAssetDocument {
    family: String,
    source_path: String,
    sha256: String,
    face_index: u32,
    pixel_height: u16,
    locales: Vec<String>,
}

impl From<FontAssetDocument> for FontAsset {
    fn from(document: FontAssetDocument) -> Self {
        Self {
            family: document.family,
            source_path: document.source_path,
            sha256: document.sha256,
            face_index: document.face_index,
            pixel_height: document.pixel_height,
            locales: document.locales,
        }
    }
}

#[derive(Clone, Debug, Deserialize)]
#[serde(deny_unknown_fields)]
struct ApprovedStringDocument {
    id: String,
    locale: String,
    value: String,
    direction: TextDirectionDocument,
}

impl ApprovedStringDocument {
    fn into_approved_string(self) -> MduxResult<ApprovedString> {
        Ok(ApprovedString {
            id: self.id,
            locale: self.locale,
            value: self.value,
            direction: self.direction.into_text_direction(),
        })
    }
}

#[derive(Clone, Copy, Debug, Deserialize)]
enum TextDirectionDocument {
    #[serde(rename = "ltr")]
    LeftToRight,
    #[serde(rename = "rtl")]
    RightToLeft,
}

impl TextDirectionDocument {
    fn into_text_direction(self) -> TextDirection {
        match self {
            Self::LeftToRight => TextDirection::LeftToRight,
            Self::RightToLeft => TextDirection::RightToLeft,
        }
    }
}

#[derive(Clone, Debug, Deserialize)]
#[serde(deny_unknown_fields)]
struct TextureAtlasDocument {
    width: u16,
    height: u16,
    pixels_hex: String,
}

impl TextureAtlasDocument {
    fn into_texture_atlas(self, package_path: &str, atlas_index: usize) -> MduxResult<TextureAtlas> {
        Ok(TextureAtlas {
            width: self.width,
            height: self.height,
            pixels: decode_hex(package_path, atlas_index, &self.pixels_hex)?,
        })
    }
}

#[derive(Clone, Debug, Deserialize)]
#[serde(deny_unknown_fields)]
struct AtlasGlyphDocument {
    atlas_index: u16,
    glyph_id: u32,
    x: u16,
    y: u16,
    width: u16,
    height: u16,
    bearing_x: i16,
    bearing_y: i16,
    advance_x: i32,
}

impl From<AtlasGlyphDocument> for AtlasGlyph {
    fn from(document: AtlasGlyphDocument) -> Self {
        Self {
            atlas_index: document.atlas_index,
            glyph_id: document.glyph_id,
            x: document.x,
            y: document.y,
            width: document.width,
            height: document.height,
            bearing_x: document.bearing_x,
            bearing_y: document.bearing_y,
            advance_x: document.advance_x,
        }
    }
}

#[derive(Clone, Debug, Deserialize)]
#[serde(deny_unknown_fields)]
struct CompiledGlyphDocument {
    atlas_index: u16,
    glyph_id: u32,
    x: i32,
    y: i32,
    advance_x: i32,
}

impl From<CompiledGlyphDocument> for CompiledGlyph {
    fn from(document: CompiledGlyphDocument) -> Self {
        Self {
            atlas_index: document.atlas_index,
            glyph_id: document.glyph_id,
            x: document.x,
            y: document.y,
            advance_x: document.advance_x,
        }
    }
}

#[derive(Clone, Debug, Deserialize)]
#[serde(deny_unknown_fields)]
struct CompiledTextRunDocument {
    id: String,
    source_string_id: String,
    locale: String,
    bidi_level: u8,
    glyphs: Vec<CompiledGlyphDocument>,
}

impl From<CompiledTextRunDocument> for CompiledTextRun {
    fn from(document: CompiledTextRunDocument) -> Self {
        Self {
            id: document.id,
            source_string_id: document.source_string_id,
            locale: document.locale,
            bidi_level: document.bidi_level,
            glyphs: document.glyphs.into_iter().map(Into::into).collect(),
        }
    }
}

#[derive(Clone, Debug, Deserialize)]
#[serde(deny_unknown_fields)]
struct NumericGlyphEntryDocument {
    character: char,
    glyph_id: u32,
    atlas_index: u16,
    advance_x: i32,
}

impl From<NumericGlyphEntryDocument> for NumericGlyphEntry {
    fn from(document: NumericGlyphEntryDocument) -> Self {
        Self {
            character: document.character,
            glyph_id: document.glyph_id,
            atlas_index: document.atlas_index,
            advance_x: document.advance_x,
        }
    }
}

#[derive(Clone, Debug, Deserialize)]
#[serde(deny_unknown_fields)]
struct NumericGlyphSetDocument {
    id: String,
    locale: String,
    entries: Vec<NumericGlyphEntryDocument>,
}

impl From<NumericGlyphSetDocument> for NumericGlyphSet {
    fn from(document: NumericGlyphSetDocument) -> Self {
        Self {
            id: document.id,
            locale: document.locale,
            entries: document.entries.into_iter().map(Into::into).collect(),
        }
    }
}

#[derive(Clone, Debug, Deserialize)]
#[serde(deny_unknown_fields)]
struct NumericTemplateDocument {
    id: String,
    locale: String,
    prefix_run_id: String,
    suffix_run_id: String,
    glyph_set_id: String,
    max_chars: u8,
    allow_negative: bool,
}

impl From<NumericTemplateDocument> for NumericTemplate {
    fn from(document: NumericTemplateDocument) -> Self {
        Self {
            id: document.id,
            locale: document.locale,
            prefix_run_id: document.prefix_run_id,
            suffix_run_id: document.suffix_run_id,
            glyph_set_id: document.glyph_set_id,
            max_chars: document.max_chars,
            allow_negative: document.allow_negative,
        }
    }
}

#[derive(Clone, Debug, Deserialize)]
#[serde(deny_unknown_fields)]
struct DeterminismEvidenceDocument {
    package_sha256: String,
    toolchain_id: String,
    unicode_version: String,
    build_recipe_sha256: String,
}

impl From<DeterminismEvidenceDocument> for DeterminismEvidence {
    fn from(document: DeterminismEvidenceDocument) -> Self {
        Self {
            package_sha256: document.package_sha256,
            toolchain_id: document.toolchain_id,
            unicode_version: document.unicode_version,
            build_recipe_sha256: document.build_recipe_sha256,
        }
    }
}

fn decode_hex(package_path: &str, atlas_index: usize, encoded: &str) -> MduxResult<Vec<u8>> {
    let bytes = encoded.as_bytes();
    if bytes.len() % 2 != 0 {
        return Err(ValidationError::new(format!(
            "text package {package_path} atlas {atlas_index} pixels_hex must have an even number of characters"
        )));
    }

    let mut decoded = Vec::with_capacity(bytes.len() / 2);
    for pair_index in 0..(bytes.len() / 2) {
        let offset = pair_index * 2;
        let high = decode_hex_nibble(package_path, atlas_index, bytes[offset], offset)?;
        let low = decode_hex_nibble(package_path, atlas_index, bytes[offset + 1], offset + 1)?;
        decoded.push((high << 4) | low);
    }

    Ok(decoded)
}

fn decode_hex_nibble(
    package_path: &str,
    atlas_index: usize,
    byte: u8,
    offset: usize,
) -> MduxResult<u8> {
    match byte {
        b'0'..=b'9' => Ok(byte - b'0'),
        b'a'..=b'f' => Ok(byte - b'a' + 10),
        b'A'..=b'F' => Ok(byte - b'A' + 10),
        _ => Err(ValidationError::new(format!(
            "text package {package_path} atlas {atlas_index} pixels_hex contains invalid hex at byte {offset}"
        ))),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn loads_default_standard_roboto_package() {
        let package = default_standard_text_package().expect("default standard package should load");

        assert_eq!(package.fonts.len(), 1);
        assert_eq!(package.fonts[0].family, DEFAULT_STANDARD_FONT.family);
        assert_eq!(package.fonts[0].pixel_height, DEFAULT_STANDARD_FONT.pixel_height);
        assert_eq!(package.fonts[0].source_path, "Roboto-Regular.ttf");
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
