#![forbid(unsafe_code)]

use arrayvec::ArrayVec;
use mdux_core::{MduxResult, Validates, ValidationError};
use mdux_text_schema::{CompiledTextRun, NumericGlyphSet, NumericTemplate, TextPackage};

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct GlyphDrawCommand {
    pub atlas_index: u16,
    pub glyph_id: u32,
    pub x: i32,
    pub y: i32,
    pub width: u16,
    pub height: u16,
}

pub struct TextRuntime<'a, const MAX_COMMANDS: usize> {
    package: &'a TextPackage,
}

impl<'a, const MAX_COMMANDS: usize> TextRuntime<'a, MAX_COMMANDS> {
    pub fn new(package: &'a TextPackage) -> MduxResult<Self> {
        package.validate()?;
        Ok(Self { package })
    }

    pub fn render_run(
        &self,
        run_id: &str,
        origin_x: i32,
        origin_y: i32,
    ) -> MduxResult<ArrayVec<GlyphDrawCommand, MAX_COMMANDS>> {
        let run = self
            .package
            .find_run(run_id)
            .ok_or_else(|| ValidationError::new("unknown text run id"))?;

        let mut commands = ArrayVec::new();
        self.append_run_commands(&mut commands, run, origin_x, origin_y)?;
        Ok(commands)
    }

    pub fn render_numeric_template(
        &self,
        template_id: &str,
        value: i64,
        origin_x: i32,
        origin_y: i32,
    ) -> MduxResult<ArrayVec<GlyphDrawCommand, MAX_COMMANDS>> {
        let template = self
            .package
            .find_template(template_id)
            .ok_or_else(|| ValidationError::new("unknown numeric template id"))?;
        let glyph_set = self
            .package
            .find_numeric_glyph_set(&template.glyph_set_id)
            .ok_or_else(|| ValidationError::new("unknown numeric glyph set"))?;

        let mut commands = ArrayVec::new();
        let prefix_run = self
            .package
            .find_run(&template.prefix_run_id)
            .ok_or_else(|| ValidationError::new("unknown prefix run"))?;
        let suffix_run = self
            .package
            .find_run(&template.suffix_run_id)
            .ok_or_else(|| ValidationError::new("unknown suffix run"))?;

        let mut cursor_x =
            self.append_run_commands(&mut commands, prefix_run, origin_x, origin_y)?;
        let numeric_chars = format_numeric_value(value, template)?;
        cursor_x = self.append_numeric_commands(
            &mut commands,
            glyph_set,
            &numeric_chars,
            cursor_x,
            origin_y,
        )?;
        self.append_run_commands(&mut commands, suffix_run, cursor_x, origin_y)?;

        Ok(commands)
    }

    fn append_run_commands(
        &self,
        commands: &mut ArrayVec<GlyphDrawCommand, MAX_COMMANDS>,
        run: &CompiledTextRun,
        origin_x: i32,
        origin_y: i32,
    ) -> MduxResult<i32> {
        for glyph in &run.glyphs {
            let atlas_glyph = self
                .package
                .find_glyph(glyph.atlas_index, glyph.glyph_id)
                .ok_or_else(|| ValidationError::new("run references unknown atlas glyph"))?;

            if atlas_glyph.width > 0 && atlas_glyph.height > 0 {
                commands
                    .try_push(GlyphDrawCommand {
                        atlas_index: glyph.atlas_index,
                        glyph_id: glyph.glyph_id,
                        x: origin_x + glyph.x,
                        y: origin_y + glyph.y,
                        width: atlas_glyph.width,
                        height: atlas_glyph.height,
                    })
                    .map_err(|_| ValidationError::new("glyph command buffer capacity exceeded"))?;
            }
        }

        Ok(origin_x + run.advance_width())
    }

    fn append_numeric_commands(
        &self,
        commands: &mut ArrayVec<GlyphDrawCommand, MAX_COMMANDS>,
        glyph_set: &NumericGlyphSet,
        characters: &[char],
        origin_x: i32,
        origin_y: i32,
    ) -> MduxResult<i32> {
        let mut cursor_x = origin_x;

        for character in characters {
            let entry = glyph_set
                .entries
                .iter()
                .find(|entry| entry.character == *character)
                .ok_or_else(|| {
                    ValidationError::new(format!(
                        "numeric glyph set does not contain character '{character}'"
                    ))
                })?;
            let atlas_glyph = self
                .package
                .find_glyph(entry.atlas_index, entry.glyph_id)
                .ok_or_else(|| {
                    ValidationError::new("numeric glyph entry references unknown glyph")
                })?;

            if atlas_glyph.width > 0 && atlas_glyph.height > 0 {
                commands
                    .try_push(GlyphDrawCommand {
                        atlas_index: entry.atlas_index,
                        glyph_id: entry.glyph_id,
                        x: cursor_x,
                        y: origin_y,
                        width: atlas_glyph.width,
                        height: atlas_glyph.height,
                    })
                    .map_err(|_| ValidationError::new("glyph command buffer capacity exceeded"))?;
            }

            cursor_x += entry.advance_x;
        }

        Ok(cursor_x)
    }
}

fn format_numeric_value(value: i64, template: &NumericTemplate) -> MduxResult<ArrayVec<char, 32>> {
    if value < 0 && !template.allow_negative {
        return Err(ValidationError::new(
            "numeric template does not allow negative values",
        ));
    }

    let mut scratch = [0u8; 32];
    let mut cursor = scratch.len();
    let mut remaining = value.unsigned_abs();

    if remaining == 0 {
        cursor -= 1;
        scratch[cursor] = b'0';
    }

    while remaining > 0 {
        cursor -= 1;
        scratch[cursor] = b'0' + (remaining % 10) as u8;
        remaining /= 10;
    }

    if value < 0 {
        cursor -= 1;
        scratch[cursor] = b'-';
    }

    if scratch.len() - cursor > usize::from(template.max_chars) {
        return Err(ValidationError::new(
            "numeric value exceeds template max_chars budget",
        ));
    }

    let mut formatted = ArrayVec::new();
    for byte in &scratch[cursor..] {
        formatted
            .try_push(char::from(*byte))
            .map_err(|_| ValidationError::new("numeric formatter buffer capacity exceeded"))?;
    }

    Ok(formatted)
}

#[cfg(test)]
mod tests {
    use mdux_text_schema::{
        ApprovedString, AtlasGlyph, CompiledGlyph, CompiledTextRun, DeterminismEvidence, FontAsset,
        NumericGlyphEntry, NumericGlyphSet, NumericTemplate, TextDirection, TextureAtlas,
    };

    use super::*;

    #[test]
    fn renders_static_run_without_heap_growth() {
        let package = example_package();
        let runtime = TextRuntime::<8>::new(&package).expect("package should validate");

        let commands = runtime
            .render_run("RUN-PREFIX", 10, 20)
            .expect("run rendering should succeed");

        assert_eq!(commands.len(), 2);
        assert_eq!(commands[0].glyph_id, 1);
        assert_eq!(commands[0].x, 10);
        assert_eq!(commands[1].x, 16);
    }

    #[test]
    fn renders_numeric_template_with_bounded_digits() {
        let package = example_package();
        let runtime = TextRuntime::<16>::new(&package).expect("package should validate");

        let commands = runtime
            .render_numeric_template("TPL-DOSE", 12, 0, 0)
            .expect("numeric template rendering should succeed");

        let glyph_ids: Vec<u32> = commands.iter().map(|command| command.glyph_id).collect();
        assert_eq!(glyph_ids, vec![1, 2, 10, 11, 3, 4]);
    }

    #[test]
    fn rejects_over_budget_numeric_value() {
        let package = example_package();
        let runtime = TextRuntime::<16>::new(&package).expect("package should validate");

        let error = runtime
            .render_numeric_template("TPL-DOSE", 12345, 0, 0)
            .expect_err("over-budget numeric value should fail");

        assert!(error.to_string().contains("max_chars"));
    }

    fn example_package() -> TextPackage {
        TextPackage {
            fonts: vec![FontAsset {
                family: "Approved Sans".to_string(),
                source_path: "fonts/approved.ttf".to_string(),
                sha256: "0123456789abcdef0123456789abcdef0123456789abcdef0123456789abcdef"
                    .to_string(),
                face_index: 0,
                pixel_height: 32,
                locales: vec!["en-US".to_string()],
            }],
            approved_strings: vec![
                ApprovedString {
                    id: "STR-PREFIX".to_string(),
                    locale: "en-US".to_string(),
                    value: "Dose ".to_string(),
                    direction: TextDirection::LeftToRight,
                },
                ApprovedString {
                    id: "STR-SUFFIX".to_string(),
                    locale: "en-US".to_string(),
                    value: "mL".to_string(),
                    direction: TextDirection::LeftToRight,
                },
            ],
            atlases: vec![TextureAtlas {
                width: 4,
                height: 4,
                pixels: vec![
                    1, 2, 3, 4, //
                    5, 6, 7, 8, //
                    9, 10, 11, 12, //
                    13, 14, 15, 16,
                ],
            }],
            atlas_glyphs: vec![
                glyph(1, 0, 0),
                glyph(2, 1, 0),
                glyph(3, 2, 0),
                glyph(4, 3, 0),
                glyph(10, 0, 1),
                glyph(11, 1, 1),
                glyph(12, 2, 1),
                glyph(13, 3, 1),
                glyph(14, 0, 2),
                glyph(15, 1, 2),
                glyph(16, 2, 2),
                glyph(20, 3, 2),
            ],
            runs: vec![
                CompiledTextRun {
                    id: "RUN-PREFIX".to_string(),
                    source_string_id: "STR-PREFIX".to_string(),
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
                },
                CompiledTextRun {
                    id: "RUN-SUFFIX".to_string(),
                    source_string_id: "STR-SUFFIX".to_string(),
                    locale: "en-US".to_string(),
                    bidi_level: 0,
                    glyphs: vec![
                        CompiledGlyph {
                            atlas_index: 0,
                            glyph_id: 3,
                            x: 0,
                            y: 0,
                            advance_x: 6,
                        },
                        CompiledGlyph {
                            atlas_index: 0,
                            glyph_id: 4,
                            x: 6,
                            y: 0,
                            advance_x: 6,
                        },
                    ],
                },
            ],
            numeric_glyph_sets: vec![NumericGlyphSet {
                id: "DIGITS".to_string(),
                locale: "en-US".to_string(),
                entries: vec![
                    NumericGlyphEntry {
                        character: '0',
                        glyph_id: 20,
                        atlas_index: 0,
                        advance_x: 6,
                    },
                    NumericGlyphEntry {
                        character: '1',
                        glyph_id: 10,
                        atlas_index: 0,
                        advance_x: 6,
                    },
                    NumericGlyphEntry {
                        character: '2',
                        glyph_id: 11,
                        atlas_index: 0,
                        advance_x: 6,
                    },
                    NumericGlyphEntry {
                        character: '3',
                        glyph_id: 12,
                        atlas_index: 0,
                        advance_x: 6,
                    },
                    NumericGlyphEntry {
                        character: '4',
                        glyph_id: 13,
                        atlas_index: 0,
                        advance_x: 6,
                    },
                    NumericGlyphEntry {
                        character: '5',
                        glyph_id: 14,
                        atlas_index: 0,
                        advance_x: 6,
                    },
                    NumericGlyphEntry {
                        character: '6',
                        glyph_id: 15,
                        atlas_index: 0,
                        advance_x: 6,
                    },
                    NumericGlyphEntry {
                        character: '7',
                        glyph_id: 16,
                        atlas_index: 0,
                        advance_x: 6,
                    },
                    NumericGlyphEntry {
                        character: '-',
                        glyph_id: 2,
                        atlas_index: 0,
                        advance_x: 6,
                    },
                ],
            }],
            numeric_templates: vec![NumericTemplate {
                id: "TPL-DOSE".to_string(),
                locale: "en-US".to_string(),
                prefix_run_id: "RUN-PREFIX".to_string(),
                suffix_run_id: "RUN-SUFFIX".to_string(),
                glyph_set_id: "DIGITS".to_string(),
                max_chars: 4,
                allow_negative: false,
            }],
            evidence: DeterminismEvidence {
                package_sha256: "0123456789abcdef0123456789abcdef0123456789abcdef0123456789abcdef"
                    .to_string(),
                toolchain_id: "rust-1.87.0".to_string(),
                unicode_version: "15.1.0".to_string(),
                build_recipe_sha256:
                    "fedcba9876543210fedcba9876543210fedcba9876543210fedcba9876543210".to_string(),
            },
        }
    }

    fn glyph(glyph_id: u32, x: u16, y: u16) -> AtlasGlyph {
        AtlasGlyph {
            atlas_index: 0,
            glyph_id,
            x,
            y,
            width: 1,
            height: 1,
            bearing_x: 0,
            bearing_y: 0,
            advance_x: 6,
        }
    }
}
