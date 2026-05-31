#![forbid(unsafe_code)]

use std::fs;
use std::path::{Path, PathBuf};

use mdux_core::{MduxResult, Validates, ValidationError, validate_non_empty};
use mdux_text_schema::{
    ApprovedString, AtlasGlyph, CompiledTextRun, DeterminismEvidence, FontAsset, NumericGlyphSet,
    NumericTemplate, TextPackage, TextureAtlas,
};
use sha2::{Digest, Sha256};

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct FontFingerprint {
    pub path: PathBuf,
    pub sha256: String,
    pub byte_len: u64,
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct RasterizedGlyph {
    pub glyph_id: u32,
    pub width: u16,
    pub height: u16,
    pub bearing_x: i16,
    pub bearing_y: i16,
    pub advance_x: i32,
    pub pixels: Vec<u8>,
}

impl Validates for RasterizedGlyph {
    fn validate(&self) -> MduxResult<()> {
        if self.pixels.len() != usize::from(self.width) * usize::from(self.height) {
            return Err(ValidationError::new(
                "rasterized glyph pixels must match width * height",
            ));
        }
        Ok(())
    }
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct TextCompilationInput {
    pub fonts: Vec<FontAsset>,
    pub approved_strings: Vec<ApprovedString>,
    pub rasterized_glyphs: Vec<RasterizedGlyph>,
    pub runs: Vec<CompiledTextRun>,
    pub numeric_glyph_sets: Vec<NumericGlyphSet>,
    pub numeric_templates: Vec<NumericTemplate>,
    pub toolchain_id: String,
    pub unicode_version: String,
    pub build_recipe: String,
    pub atlas_width: u16,
    pub atlas_padding: u16,
}

impl Validates for TextCompilationInput {
    fn validate(&self) -> MduxResult<()> {
        if self.fonts.is_empty() {
            return Err(ValidationError::new(
                "text compilation input must contain at least one font",
            ));
        }
        if self.approved_strings.is_empty() {
            return Err(ValidationError::new(
                "text compilation input must contain at least one approved string",
            ));
        }
        if self.rasterized_glyphs.is_empty() {
            return Err(ValidationError::new(
                "text compilation input must contain at least one rasterized glyph",
            ));
        }
        if self.runs.is_empty() {
            return Err(ValidationError::new(
                "text compilation input must contain at least one compiled run",
            ));
        }
        validate_non_empty("toolchain_id", &self.toolchain_id)?;
        validate_non_empty("unicode_version", &self.unicode_version)?;
        validate_non_empty("build_recipe", &self.build_recipe)?;
        if self.atlas_width == 0 {
            return Err(ValidationError::new("atlas_width must be positive"));
        }

        for font in &self.fonts {
            font.validate()?;
        }
        for approved_string in &self.approved_strings {
            approved_string.validate()?;
        }
        for glyph in &self.rasterized_glyphs {
            glyph.validate()?;
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

        Ok(())
    }
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct DeterministicAtlasBuilder {
    atlas_width: u16,
    padding: u16,
}

impl DeterministicAtlasBuilder {
    pub fn new(atlas_width: u16, padding: u16) -> Self {
        Self {
            atlas_width,
            padding,
        }
    }

    pub fn build(&self, glyphs: &[RasterizedGlyph]) -> MduxResult<(TextureAtlas, Vec<AtlasGlyph>)> {
        if glyphs.is_empty() {
            return Err(ValidationError::new(
                "deterministic atlas builder requires at least one glyph",
            ));
        }

        let mut ordered_glyphs = glyphs.to_vec();
        ordered_glyphs.sort_by_key(|glyph| glyph.glyph_id);

        for glyph in &ordered_glyphs {
            glyph.validate()?;
            if usize::from(glyph.width) + usize::from(self.padding) * 2
                > usize::from(self.atlas_width)
            {
                return Err(ValidationError::new(
                    "glyph width exceeds deterministic atlas width budget",
                ));
            }
        }

        let padding = usize::from(self.padding);
        let atlas_width = usize::from(self.atlas_width);
        let mut placements = Vec::with_capacity(ordered_glyphs.len());
        let mut cursor_x = padding;
        let mut cursor_y = padding;
        let mut row_height = 0usize;

        for glyph in &ordered_glyphs {
            let glyph_width = usize::from(glyph.width);
            let glyph_height = usize::from(glyph.height);

            if cursor_x + glyph_width + padding > atlas_width {
                cursor_x = padding;
                cursor_y += row_height + padding;
                row_height = 0;
            }

            placements.push((cursor_x, cursor_y));
            cursor_x += glyph_width + padding;
            row_height = row_height.max(glyph_height);
        }

        let atlas_height = (cursor_y + row_height + padding).max(1);
        if atlas_height > usize::from(u16::MAX) {
            return Err(ValidationError::new(
                "deterministic atlas height exceeds supported u16 range",
            ));
        }

        let mut pixels = vec![0u8; atlas_width * atlas_height];
        let mut atlas_glyphs = Vec::with_capacity(ordered_glyphs.len());

        for (glyph, (x, y)) in ordered_glyphs.iter().zip(placements.iter().copied()) {
            blit_glyph(&mut pixels, atlas_width, glyph, x, y);
            atlas_glyphs.push(AtlasGlyph {
                atlas_index: 0,
                glyph_id: glyph.glyph_id,
                x: x as u16,
                y: y as u16,
                width: glyph.width,
                height: glyph.height,
                bearing_x: glyph.bearing_x,
                bearing_y: glyph.bearing_y,
                advance_x: glyph.advance_x,
            });
        }

        Ok((
            TextureAtlas {
                width: self.atlas_width,
                height: atlas_height as u16,
                pixels,
            },
            atlas_glyphs,
        ))
    }
}

pub fn fingerprint_font_file(path: impl AsRef<Path>) -> MduxResult<FontFingerprint> {
    let path = path.as_ref();
    let contents = fs::read(path).map_err(|error| {
        ValidationError::new(format!(
            "failed to read font file {}: {error}",
            path.display()
        ))
    })?;

    Ok(FontFingerprint {
        path: path.to_path_buf(),
        sha256: sha256_bytes(&contents),
        byte_len: contents.len() as u64,
    })
}

pub fn compile_text_package(input: TextCompilationInput) -> MduxResult<TextPackage> {
    input.validate()?;

    let TextCompilationInput {
        mut fonts,
        mut approved_strings,
        rasterized_glyphs,
        mut runs,
        mut numeric_glyph_sets,
        mut numeric_templates,
        toolchain_id,
        unicode_version,
        build_recipe,
        atlas_width,
        atlas_padding,
    } = input;

    fonts.sort_by(|left, right| {
        left.family
            .cmp(&right.family)
            .then_with(|| left.source_path.cmp(&right.source_path))
    });
    approved_strings.sort_by(|left, right| {
        left.locale
            .cmp(&right.locale)
            .then_with(|| left.id.cmp(&right.id))
    });
    runs.sort_by(|left, right| left.id.cmp(&right.id));
    numeric_glyph_sets.sort_by(|left, right| left.id.cmp(&right.id));
    for glyph_set in &mut numeric_glyph_sets {
        glyph_set.entries.sort_by_key(|entry| entry.character);
    }
    numeric_templates.sort_by(|left, right| left.id.cmp(&right.id));

    let atlas_builder = DeterministicAtlasBuilder::new(atlas_width, atlas_padding);
    let (atlas, atlas_glyphs) = atlas_builder.build(&rasterized_glyphs)?;

    let recipe_hash = sha256_text(&build_recipe);
    let package_hash = canonical_package_hash(
        &fonts,
        &approved_strings,
        &atlas,
        &atlas_glyphs,
        &runs,
        &numeric_glyph_sets,
        &numeric_templates,
        &toolchain_id,
        &unicode_version,
        &recipe_hash,
    );

    let package = TextPackage {
        fonts,
        approved_strings,
        atlases: vec![atlas],
        atlas_glyphs,
        runs,
        numeric_glyph_sets,
        numeric_templates,
        evidence: DeterminismEvidence {
            package_sha256: package_hash,
            toolchain_id,
            unicode_version,
            build_recipe_sha256: recipe_hash,
        },
    };
    package.validate()?;
    Ok(package)
}

fn blit_glyph(
    atlas_pixels: &mut [u8],
    atlas_width: usize,
    glyph: &RasterizedGlyph,
    x: usize,
    y: usize,
) {
    let glyph_width = usize::from(glyph.width);
    let glyph_height = usize::from(glyph.height);

    for row in 0..glyph_height {
        let source_start = row * glyph_width;
        let source_end = source_start + glyph_width;
        let destination_start = (y + row) * atlas_width + x;
        let destination_end = destination_start + glyph_width;
        atlas_pixels[destination_start..destination_end]
            .copy_from_slice(&glyph.pixels[source_start..source_end]);
    }
}

fn canonical_package_hash(
    fonts: &[FontAsset],
    approved_strings: &[ApprovedString],
    atlas: &TextureAtlas,
    atlas_glyphs: &[AtlasGlyph],
    runs: &[CompiledTextRun],
    numeric_glyph_sets: &[NumericGlyphSet],
    numeric_templates: &[NumericTemplate],
    toolchain_id: &str,
    unicode_version: &str,
    recipe_hash: &str,
) -> String {
    let mut canonical = String::new();

    for font in fonts {
        canonical.push_str(&format!(
            "font|{}|{}|{}|{}|{}|{}\n",
            font.family,
            font.source_path,
            font.sha256,
            font.face_index,
            font.pixel_height,
            font.locales.join(",")
        ));
    }
    for approved_string in approved_strings {
        canonical.push_str(&format!(
            "string|{}|{}|{:?}|{}\n",
            approved_string.id,
            approved_string.locale,
            approved_string.direction,
            approved_string.value
        ));
    }

    canonical.push_str(&format!(
        "atlas|{}|{}|{}\n",
        atlas.width,
        atlas.height,
        sha256_bytes(&atlas.pixels)
    ));

    for glyph in atlas_glyphs {
        canonical.push_str(&format!(
            "glyph|{}|{}|{}|{}|{}|{}|{}|{}|{}\n",
            glyph.atlas_index,
            glyph.glyph_id,
            glyph.x,
            glyph.y,
            glyph.width,
            glyph.height,
            glyph.bearing_x,
            glyph.bearing_y,
            glyph.advance_x
        ));
    }

    for run in runs {
        canonical.push_str(&format!(
            "run|{}|{}|{}|{}\n",
            run.id, run.source_string_id, run.locale, run.bidi_level
        ));
        for glyph in &run.glyphs {
            canonical.push_str(&format!(
                "run-glyph|{}|{}|{}|{}|{}\n",
                glyph.atlas_index, glyph.glyph_id, glyph.x, glyph.y, glyph.advance_x
            ));
        }
    }

    for glyph_set in numeric_glyph_sets {
        canonical.push_str(&format!(
            "glyph-set|{}|{}\n",
            glyph_set.id, glyph_set.locale
        ));
        for entry in &glyph_set.entries {
            canonical.push_str(&format!(
                "glyph-entry|{}|{}|{}|{}\n",
                entry.character, entry.glyph_id, entry.atlas_index, entry.advance_x
            ));
        }
    }

    for template in numeric_templates {
        canonical.push_str(&format!(
            "template|{}|{}|{}|{}|{}|{}|{}\n",
            template.id,
            template.locale,
            template.prefix_run_id,
            template.suffix_run_id,
            template.glyph_set_id,
            template.max_chars,
            template.allow_negative
        ));
    }

    canonical.push_str(&format!(
        "evidence|{}|{}|{}\n",
        toolchain_id, unicode_version, recipe_hash
    ));

    sha256_text(&canonical)
}

fn sha256_text(value: &str) -> String {
    sha256_bytes(value.as_bytes())
}

fn sha256_bytes(value: &[u8]) -> String {
    let digest = Sha256::digest(value);
    let mut encoded = String::with_capacity(digest.len() * 2);
    for byte in digest {
        encoded.push_str(&format!("{byte:02x}"));
    }
    encoded
}

pub fn pipeline_description() -> &'static str {
    "font-intake -> catalog-compile -> deterministic-atlas -> package-verify"
}

#[cfg(test)]
mod tests {
    use std::time::{SystemTime, UNIX_EPOCH};

    use mdux_text_schema::{CompiledGlyph, NumericGlyphEntry, TextDirection};

    use super::*;

    #[test]
    fn fingerprints_font_file_contents() {
        let unique = SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .expect("system time should be after unix epoch")
            .as_nanos();
        let path = std::env::temp_dir().join(format!("mdux-font-{unique}.bin"));
        fs::write(&path, b"approved-font").expect("temporary font fixture should be writable");

        let fingerprint = fingerprint_font_file(&path).expect("font hashing should succeed");

        assert_eq!(fingerprint.byte_len, 13);
        assert_eq!(
            fingerprint.sha256,
            "1dfea108075e76e3246ac5e416b58d3ef9a4e24e46daf651af5ec16a61c21743"
        );

        fs::remove_file(path).expect("temporary font fixture should be removable");
    }

    #[test]
    fn compiles_package_deterministically() {
        let first = compile_text_package(example_input()).expect("first compile should succeed");
        let mut reordered = example_input();
        reordered.rasterized_glyphs.reverse();
        let second = compile_text_package(reordered)
            .expect("second compile with re-ordered glyphs should succeed");

        assert_eq!(
            first.evidence.package_sha256,
            second.evidence.package_sha256
        );
        assert_eq!(first.atlases[0].pixels, second.atlases[0].pixels);
    }

    fn example_input() -> TextCompilationInput {
        TextCompilationInput {
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
                id: "STR-DOSE".to_string(),
                locale: "en-US".to_string(),
                value: "Dose".to_string(),
                direction: TextDirection::LeftToRight,
            }],
            rasterized_glyphs: vec![
                RasterizedGlyph {
                    glyph_id: 2,
                    width: 1,
                    height: 1,
                    bearing_x: 0,
                    bearing_y: 0,
                    advance_x: 6,
                    pixels: vec![20],
                },
                RasterizedGlyph {
                    glyph_id: 1,
                    width: 1,
                    height: 1,
                    bearing_x: 0,
                    bearing_y: 0,
                    advance_x: 6,
                    pixels: vec![10],
                },
            ],
            runs: vec![CompiledTextRun {
                id: "RUN-DOSE".to_string(),
                source_string_id: "STR-DOSE".to_string(),
                locale: "en-US".to_string(),
                bidi_level: 0,
                glyphs: vec![
                    CompiledGlyph {
                        atlas_index: 0,
                        glyph_id: 1,
                        x: 0,
                        y: 0,
                        advance_x: 6,
                    },
                    CompiledGlyph {
                        atlas_index: 0,
                        glyph_id: 2,
                        x: 6,
                        y: 0,
                        advance_x: 6,
                    },
                ],
            }],
            numeric_glyph_sets: vec![NumericGlyphSet {
                id: "DIGITS".to_string(),
                locale: "en-US".to_string(),
                entries: vec![NumericGlyphEntry {
                    character: '1',
                    glyph_id: 1,
                    atlas_index: 0,
                    advance_x: 6,
                }],
            }],
            numeric_templates: vec![NumericTemplate {
                id: "TPL-DOSE".to_string(),
                locale: "en-US".to_string(),
                prefix_run_id: "RUN-DOSE".to_string(),
                suffix_run_id: "RUN-DOSE".to_string(),
                glyph_set_id: "DIGITS".to_string(),
                max_chars: 4,
                allow_negative: false,
            }],
            toolchain_id: "rust-1.87.0+locked".to_string(),
            unicode_version: "15.1.0".to_string(),
            build_recipe: "approved-font+approved-strings".to_string(),
            atlas_width: 8,
            atlas_padding: 1,
        }
    }
}
