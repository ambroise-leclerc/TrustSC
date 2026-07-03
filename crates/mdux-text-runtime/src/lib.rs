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
        let prefix_run = template
            .prefix_run_id
            .as_deref()
            .map(|run_id| {
                self.package
                    .find_run(run_id)
                    .ok_or_else(|| ValidationError::new("unknown prefix run"))
            })
            .transpose()?;
        let suffix_run = template
            .suffix_run_id
            .as_deref()
            .map(|run_id| {
                self.package
                    .find_run(run_id)
                    .ok_or_else(|| ValidationError::new("unknown suffix run"))
            })
            .transpose()?;

        let mut cursor_x = origin_x;
        if let Some(prefix_run) = prefix_run {
            cursor_x = self.append_run_commands(&mut commands, prefix_run, cursor_x, origin_y)?;
        }
        let numeric_chars = format_numeric_value(value, template)?;
        cursor_x = self.append_numeric_commands(
            &mut commands,
            glyph_set,
            &numeric_chars,
            cursor_x,
            origin_y,
        )?;
        if let Some(suffix_run) = suffix_run {
            self.append_run_commands(&mut commands, suffix_run, cursor_x, origin_y)?;
        }

        Ok(commands)
    }

    /// Renders a wall-clock time as `HH:MM:SS` (eight glyphs, zero-padded) from a numeric glyph
    /// set that must contain the ten digits and `:`. Bounded like every other runtime path: no
    /// allocation, capacity enforced by `MAX_COMMANDS`.
    pub fn render_clock(
        &self,
        glyph_set_id: &str,
        hours: u8,
        minutes: u8,
        seconds: u8,
        origin_x: i32,
        origin_y: i32,
    ) -> MduxResult<ArrayVec<GlyphDrawCommand, MAX_COMMANDS>> {
        if hours > 23 {
            return Err(ValidationError::new("clock hours must be in 0..=23"));
        }
        if minutes > 59 || seconds > 59 {
            return Err(ValidationError::new(
                "clock minutes and seconds must be in 0..=59",
            ));
        }

        let characters: ArrayVec<char, 8> = [
            digit_char(hours / 10),
            digit_char(hours % 10),
            ':',
            digit_char(minutes / 10),
            digit_char(minutes % 10),
            ':',
            digit_char(seconds / 10),
            digit_char(seconds % 10),
        ]
        .into_iter()
        .collect();

        self.render_glyph_set_characters(glyph_set_id, &characters, origin_x, origin_y)
    }

    /// Renders a civil date as `YYYY-MM-DD` (ten glyphs, zero-padded) from a numeric glyph set
    /// that must contain the ten digits and `-`. Bounded; years outside 0..=9999 are rejected.
    pub fn render_date(
        &self,
        glyph_set_id: &str,
        year: u16,
        month: u8,
        day: u8,
        origin_x: i32,
        origin_y: i32,
    ) -> MduxResult<ArrayVec<GlyphDrawCommand, MAX_COMMANDS>> {
        if year > 9999 {
            return Err(ValidationError::new("date year must be in 0..=9999"));
        }
        if !(1..=12).contains(&month) {
            return Err(ValidationError::new("date month must be in 1..=12"));
        }
        if !(1..=31).contains(&day) {
            return Err(ValidationError::new("date day must be in 1..=31"));
        }

        let characters: ArrayVec<char, 10> = [
            digit_char((year / 1000) as u8),
            digit_char((year / 100 % 10) as u8),
            digit_char((year / 10 % 10) as u8),
            digit_char((year % 10) as u8),
            '-',
            digit_char(month / 10),
            digit_char(month % 10),
            '-',
            digit_char(day / 10),
            digit_char(day % 10),
        ]
        .into_iter()
        .collect();

        self.render_glyph_set_characters(glyph_set_id, &characters, origin_x, origin_y)
    }

    fn render_glyph_set_characters(
        &self,
        glyph_set_id: &str,
        characters: &[char],
        origin_x: i32,
        origin_y: i32,
    ) -> MduxResult<ArrayVec<GlyphDrawCommand, MAX_COMMANDS>> {
        let glyph_set = self
            .package
            .find_numeric_glyph_set(glyph_set_id)
            .ok_or_else(|| ValidationError::new("unknown numeric glyph set"))?;

        let mut commands = ArrayVec::new();
        self.append_numeric_commands(&mut commands, glyph_set, characters, origin_x, origin_y)?;
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

fn digit_char(value: u8) -> char {
    char::from(b'0' + (value % 10))
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

    #[test]
    fn renders_affixless_template_digits_only() {
        let package = example_package();
        let runtime = TextRuntime::<16>::new(&package).expect("package should validate");

        let commands = runtime
            .render_numeric_template("TPL-SCORE", 42, 100, 50)
            .expect("digits-only template should render");

        let glyph_ids: Vec<u32> = commands.iter().map(|command| command.glyph_id).collect();
        assert_eq!(glyph_ids, vec![13, 11]); // '4' then '2', no affix glyphs
        assert_eq!(commands[0].x, 100); // digits start exactly at the origin
        assert_eq!(commands[1].x, 106);
        assert_eq!(commands[0].y, 50);
    }

    #[test]
    fn renders_clock_as_eight_zero_padded_glyphs() {
        let package = example_package();
        let runtime = TextRuntime::<16>::new(&package).expect("package should validate");

        let commands = runtime
            .render_clock("DIGITS", 7, 5, 30, 0, 0)
            .expect("clock rendering should succeed");

        // "07:05:30" — glyph ids: 0→20, 7→16, :→3, 0→20, 5→14, :→3, 3→12, 0→20
        let glyph_ids: Vec<u32> = commands.iter().map(|command| command.glyph_id).collect();
        assert_eq!(glyph_ids, vec![20, 16, 3, 20, 14, 3, 12, 20]);
        let xs: Vec<i32> = commands.iter().map(|command| command.x).collect();
        assert_eq!(xs, vec![0, 6, 12, 18, 24, 30, 36, 42]); // fixed 8-glyph advance
    }

    #[test]
    fn renders_date_as_ten_zero_padded_glyphs() {
        let package = example_package();
        let runtime = TextRuntime::<16>::new(&package).expect("package should validate");

        let commands = runtime
            .render_date("DIGITS", 2026, 7, 3, 0, 0)
            .expect("date rendering should succeed");

        // "2026-07-03" — '2'→11, '0'→20, '2'→11, '6'→15, '-'→2, '0'→20, '7'→16, '-'→2, '0'→20, '3'→12
        let glyph_ids: Vec<u32> = commands.iter().map(|command| command.glyph_id).collect();
        assert_eq!(glyph_ids, vec![11, 20, 11, 15, 2, 20, 16, 2, 20, 12]);
    }

    #[test]
    fn rejects_out_of_range_clock_and_date_values() {
        let package = example_package();
        let runtime = TextRuntime::<16>::new(&package).expect("package should validate");

        let error = runtime
            .render_clock("DIGITS", 24, 0, 0, 0, 0)
            .expect_err("hour 24 should be rejected");
        assert!(error.to_string().contains("0..=23"));

        let error = runtime
            .render_clock("DIGITS", 0, 60, 0, 0, 0)
            .expect_err("minute 60 should be rejected");
        assert!(error.to_string().contains("0..=59"));

        let error = runtime
            .render_date("DIGITS", 2026, 13, 1, 0, 0)
            .expect_err("month 13 should be rejected");
        assert!(error.to_string().contains("1..=12"));
    }

    #[test]
    fn rejects_clock_when_glyph_set_lacks_a_character() {
        let mut package = example_package();
        package.numeric_glyph_sets[0]
            .entries
            .retain(|entry| entry.character != ':');
        let runtime = TextRuntime::<16>::new(&package).expect("package should validate");

        let error = runtime
            .render_clock("DIGITS", 1, 2, 3, 0, 0)
            .expect_err("missing ':' glyph should be rejected");

        assert!(error.to_string().contains("':'"));
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
                    NumericGlyphEntry {
                        character: ':',
                        glyph_id: 3,
                        atlas_index: 0,
                        advance_x: 6,
                    },
                ],
            }],
            numeric_templates: vec![
                NumericTemplate {
                    id: "TPL-DOSE".to_string(),
                    locale: "en-US".to_string(),
                    prefix_run_id: Some("RUN-PREFIX".to_string()),
                    suffix_run_id: Some("RUN-SUFFIX".to_string()),
                    glyph_set_id: "DIGITS".to_string(),
                    max_chars: 4,
                    allow_negative: false,
                },
                NumericTemplate {
                    id: "TPL-SCORE".to_string(),
                    locale: "en-US".to_string(),
                    prefix_run_id: None,
                    suffix_run_id: None,
                    glyph_set_id: "DIGITS".to_string(),
                    max_chars: 2,
                    allow_negative: false,
                },
            ],
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
