#![forbid(unsafe_code)]

use std::collections::BTreeSet;

use mdux_core::{MduxResult, Validates, ValidationError, validate_non_empty};

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum TextDirection {
    LeftToRight,
    RightToLeft,
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct FontAsset {
    pub family: String,
    pub source_path: String,
    pub sha256: String,
    pub face_index: u32,
    pub pixel_height: u16,
    pub locales: Vec<String>,
}

impl Validates for FontAsset {
    fn validate(&self) -> MduxResult<()> {
        validate_non_empty("font family", &self.family)?;
        validate_non_empty("font source_path", &self.source_path)?;
        validate_non_empty("font sha256", &self.sha256)?;
        if !is_sha256(&self.sha256) {
            return Err(ValidationError::new(
                "font sha256 must be a 64-character lowercase hexadecimal digest",
            ));
        }
        if self.pixel_height == 0 {
            return Err(ValidationError::new("font pixel_height must be positive"));
        }
        if self.locales.is_empty() {
            return Err(ValidationError::new(
                "font asset must declare at least one approved locale",
            ));
        }
        for locale in &self.locales {
            validate_non_empty("font locale", locale)?;
        }
        Ok(())
    }
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct ApprovedString {
    pub id: String,
    pub locale: String,
    pub value: String,
    pub direction: TextDirection,
}

impl Validates for ApprovedString {
    fn validate(&self) -> MduxResult<()> {
        validate_non_empty("approved string id", &self.id)?;
        validate_non_empty("approved string locale", &self.locale)?;
        validate_non_empty("approved string value", &self.value)?;
        Ok(())
    }
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct TextureAtlas {
    pub width: u16,
    pub height: u16,
    pub pixels: Vec<u8>,
}

impl Validates for TextureAtlas {
    fn validate(&self) -> MduxResult<()> {
        if self.width == 0 || self.height == 0 {
            return Err(ValidationError::new(
                "atlas dimensions must be strictly positive",
            ));
        }
        if self.pixels.len() != usize::from(self.width) * usize::from(self.height) {
            return Err(ValidationError::new(
                "atlas pixels must match width * height",
            ));
        }
        Ok(())
    }
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct AtlasGlyph {
    pub atlas_index: u16,
    pub glyph_id: u32,
    pub x: u16,
    pub y: u16,
    pub width: u16,
    pub height: u16,
    pub bearing_x: i16,
    pub bearing_y: i16,
    pub advance_x: i32,
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct CompiledGlyph {
    pub atlas_index: u16,
    pub glyph_id: u32,
    pub x: i32,
    pub y: i32,
    pub advance_x: i32,
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct CompiledTextRun {
    pub id: String,
    pub source_string_id: String,
    pub locale: String,
    pub bidi_level: u8,
    pub glyphs: Vec<CompiledGlyph>,
}

impl CompiledTextRun {
    pub fn advance_width(&self) -> i32 {
        self.glyphs.iter().map(|glyph| glyph.advance_x).sum()
    }
}

impl Validates for CompiledTextRun {
    fn validate(&self) -> MduxResult<()> {
        validate_non_empty("compiled text run id", &self.id)?;
        validate_non_empty("compiled text run source_string_id", &self.source_string_id)?;
        validate_non_empty("compiled text run locale", &self.locale)?;
        if self.glyphs.is_empty() {
            return Err(ValidationError::new(
                "compiled text run must contain at least one glyph",
            ));
        }
        Ok(())
    }
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct NumericGlyphEntry {
    pub character: char,
    pub glyph_id: u32,
    pub atlas_index: u16,
    pub advance_x: i32,
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct NumericGlyphSet {
    pub id: String,
    pub locale: String,
    pub entries: Vec<NumericGlyphEntry>,
}

impl Validates for NumericGlyphSet {
    fn validate(&self) -> MduxResult<()> {
        validate_non_empty("numeric glyph set id", &self.id)?;
        validate_non_empty("numeric glyph set locale", &self.locale)?;
        if self.entries.is_empty() {
            return Err(ValidationError::new(
                "numeric glyph set must contain at least one entry",
            ));
        }

        let mut characters = BTreeSet::new();
        for entry in &self.entries {
            if !characters.insert(entry.character) {
                return Err(ValidationError::new(
                    "numeric glyph set contains duplicate characters",
                ));
            }
        }

        Ok(())
    }
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct NumericTemplate {
    pub id: String,
    pub locale: String,
    pub prefix_run_id: String,
    pub suffix_run_id: String,
    pub glyph_set_id: String,
    pub max_chars: u8,
    pub allow_negative: bool,
}

impl Validates for NumericTemplate {
    fn validate(&self) -> MduxResult<()> {
        validate_non_empty("numeric template id", &self.id)?;
        validate_non_empty("numeric template locale", &self.locale)?;
        validate_non_empty("numeric template prefix_run_id", &self.prefix_run_id)?;
        validate_non_empty("numeric template suffix_run_id", &self.suffix_run_id)?;
        validate_non_empty("numeric template glyph_set_id", &self.glyph_set_id)?;
        if self.max_chars == 0 {
            return Err(ValidationError::new(
                "numeric template max_chars must be positive",
            ));
        }
        Ok(())
    }
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct DeterminismEvidence {
    pub package_sha256: String,
    pub toolchain_id: String,
    pub unicode_version: String,
    pub build_recipe_sha256: String,
}

impl Validates for DeterminismEvidence {
    fn validate(&self) -> MduxResult<()> {
        validate_non_empty("package sha256", &self.package_sha256)?;
        validate_non_empty("toolchain_id", &self.toolchain_id)?;
        validate_non_empty("unicode_version", &self.unicode_version)?;
        validate_non_empty("build_recipe_sha256", &self.build_recipe_sha256)?;

        if !is_sha256(&self.package_sha256) || !is_sha256(&self.build_recipe_sha256) {
            return Err(ValidationError::new(
                "determinism evidence digests must be 64-character lowercase hexadecimal values",
            ));
        }

        Ok(())
    }
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct TextPackage {
    pub fonts: Vec<FontAsset>,
    pub approved_strings: Vec<ApprovedString>,
    pub atlases: Vec<TextureAtlas>,
    pub atlas_glyphs: Vec<AtlasGlyph>,
    pub runs: Vec<CompiledTextRun>,
    pub numeric_glyph_sets: Vec<NumericGlyphSet>,
    pub numeric_templates: Vec<NumericTemplate>,
    pub evidence: DeterminismEvidence,
}

impl TextPackage {
    pub fn find_run(&self, run_id: &str) -> Option<&CompiledTextRun> {
        self.runs.iter().find(|run| run.id == run_id)
    }

    pub fn find_template(&self, template_id: &str) -> Option<&NumericTemplate> {
        self.numeric_templates
            .iter()
            .find(|template| template.id == template_id)
    }

    pub fn find_numeric_glyph_set(&self, glyph_set_id: &str) -> Option<&NumericGlyphSet> {
        self.numeric_glyph_sets
            .iter()
            .find(|glyph_set| glyph_set.id == glyph_set_id)
    }

    pub fn find_glyph(&self, atlas_index: u16, glyph_id: u32) -> Option<&AtlasGlyph> {
        self.atlas_glyphs
            .iter()
            .find(|glyph| glyph.atlas_index == atlas_index && glyph.glyph_id == glyph_id)
    }
}

impl Validates for TextPackage {
    fn validate(&self) -> MduxResult<()> {
        if self.fonts.is_empty() {
            return Err(ValidationError::new(
                "text package must contain at least one font asset",
            ));
        }
        if self.approved_strings.is_empty() {
            return Err(ValidationError::new(
                "text package must contain at least one approved string",
            ));
        }
        if self.atlases.is_empty() {
            return Err(ValidationError::new(
                "text package must contain at least one atlas",
            ));
        }
        if self.runs.is_empty() {
            return Err(ValidationError::new(
                "text package must contain at least one compiled run",
            ));
        }

        for font in &self.fonts {
            font.validate()?;
        }
        for approved_string in &self.approved_strings {
            approved_string.validate()?;
        }
        for atlas in &self.atlases {
            atlas.validate()?;
        }
        for run in &self.runs {
            run.validate()?;
        }
        for glyph_set in &self.numeric_glyph_sets {
            glyph_set.validate()?;
        }
        for template in &self.numeric_templates {
            template.validate()?;
        }
        self.evidence.validate()?;

        ensure_unique_ids(
            self.approved_strings.iter().map(|entry| entry.id.as_str()),
            "approved string",
        )?;
        ensure_unique_ids(
            self.runs.iter().map(|entry| entry.id.as_str()),
            "compiled run",
        )?;
        ensure_unique_ids(
            self.numeric_glyph_sets
                .iter()
                .map(|entry| entry.id.as_str()),
            "numeric glyph set",
        )?;
        ensure_unique_ids(
            self.numeric_templates.iter().map(|entry| entry.id.as_str()),
            "numeric template",
        )?;

        for run in &self.runs {
            if !self
                .approved_strings
                .iter()
                .any(|approved_string| approved_string.id == run.source_string_id)
            {
                return Err(ValidationError::new(
                    "compiled run references an unknown approved string",
                ));
            }

            for glyph in &run.glyphs {
                if self.find_glyph(glyph.atlas_index, glyph.glyph_id).is_none() {
                    return Err(ValidationError::new(
                        "compiled run references an unknown atlas glyph",
                    ));
                }
            }
        }

        for glyph_set in &self.numeric_glyph_sets {
            for entry in &glyph_set.entries {
                if self.find_glyph(entry.atlas_index, entry.glyph_id).is_none() {
                    return Err(ValidationError::new(
                        "numeric glyph set references an unknown atlas glyph",
                    ));
                }
            }
        }

        for template in &self.numeric_templates {
            if self.find_run(&template.prefix_run_id).is_none()
                || self.find_run(&template.suffix_run_id).is_none()
            {
                return Err(ValidationError::new(
                    "numeric template references an unknown run",
                ));
            }
            if self
                .find_numeric_glyph_set(&template.glyph_set_id)
                .is_none()
            {
                return Err(ValidationError::new(
                    "numeric template references an unknown numeric glyph set",
                ));
            }
        }

        Ok(())
    }
}

fn ensure_unique_ids<'a>(ids: impl IntoIterator<Item = &'a str>, label: &str) -> MduxResult<()> {
    let mut seen = BTreeSet::new();
    for id in ids {
        if !seen.insert(id.to_string()) {
            return Err(ValidationError::new(format!("{label} ids must be unique")));
        }
    }
    Ok(())
}

fn is_sha256(value: &str) -> bool {
    value.len() == 64
        && value
            .bytes()
            .all(|byte| byte.is_ascii_hexdigit() && !byte.is_ascii_uppercase())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn rejects_invalid_sha256() {
        let font = FontAsset {
            family: "Approved Sans".to_string(),
            source_path: "fonts/approved.ttf".to_string(),
            sha256: "abc".to_string(),
            face_index: 0,
            pixel_height: 32,
            locales: vec!["en-US".to_string()],
        };

        assert!(font.validate().is_err());
    }

    #[test]
    fn validates_minimal_package() {
        let package = TextPackage {
            fonts: vec![FontAsset {
                family: "Approved Sans".to_string(),
                source_path: "fonts/approved.ttf".to_string(),
                sha256: "0123456789abcdef0123456789abcdef0123456789abcdef0123456789abcdef"
                    .to_string(),
                face_index: 0,
                pixel_height: 32,
                locales: vec!["en-US".to_string()],
            }],
            approved_strings: vec![ApprovedString {
                id: "STR-HELLO".to_string(),
                locale: "en-US".to_string(),
                value: "Hello".to_string(),
                direction: TextDirection::LeftToRight,
            }],
            atlases: vec![TextureAtlas {
                width: 2,
                height: 2,
                pixels: vec![1, 2, 3, 4],
            }],
            atlas_glyphs: vec![AtlasGlyph {
                atlas_index: 0,
                glyph_id: 1,
                x: 0,
                y: 0,
                width: 1,
                height: 1,
                bearing_x: 0,
                bearing_y: 0,
                advance_x: 8,
            }],
            runs: vec![CompiledTextRun {
                id: "RUN-HELLO".to_string(),
                source_string_id: "STR-HELLO".to_string(),
                locale: "en-US".to_string(),
                bidi_level: 0,
                glyphs: vec![CompiledGlyph {
                    atlas_index: 0,
                    glyph_id: 1,
                    x: 0,
                    y: 0,
                    advance_x: 8,
                }],
            }],
            numeric_glyph_sets: vec![],
            numeric_templates: vec![],
            evidence: DeterminismEvidence {
                package_sha256: "0123456789abcdef0123456789abcdef0123456789abcdef0123456789abcdef"
                    .to_string(),
                toolchain_id: "rust-1.87.0".to_string(),
                unicode_version: "15.1.0".to_string(),
                build_recipe_sha256:
                    "fedcba9876543210fedcba9876543210fedcba9876543210fedcba9876543210".to_string(),
            },
        };

        assert!(package.validate().is_ok());
    }
}
