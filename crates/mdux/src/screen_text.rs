//! Joins a compiled MedUI screen (`CompiledScreenPackage`) with an approved `TextPackage` to
//! produce per-node glyph draw commands, so applications and presentation adapters never need to
//! hand-roll the "resolve node text key -> compiled run -> glyph origin -> draw commands" glue
//! that `examples/hello_world` originally wrote inline.

use crate::{
    CompiledScreenPackage, GlyphDrawCommand, MduxResult, TextPackage, TextRuntime, ValidationError,
};

/// Upper bound on glyph commands rendered for a single text run. This is one-time startup work
/// in an allocating crate (unlike the bounded, no-alloc `TextRuntime` consumers on-device), so
/// exceeding it is a configuration error, not a runtime budget violation.
pub const MAX_GLYPH_COMMANDS_PER_RUN: usize = 256;

/// The resolved glyph draw commands for a single screen node's text.
#[derive(Clone, Debug, PartialEq)]
pub struct ScreenTextRun {
    pub node_id: &'static str,
    pub run_id: String,
    pub origin_x: i32,
    pub origin_y: i32,
    pub commands: Vec<GlyphDrawCommand>,
}

/// Every text run resolved from a compiled screen's nodes, alongside the text package they were
/// resolved against (so callers can access atlases, fonts, etc. without loading it twice).
#[derive(Clone, Debug, PartialEq)]
pub struct ScreenTextLayout {
    pub package: TextPackage,
    pub runs: Vec<ScreenTextRun>,
}

impl ScreenTextLayout {
    /// Resolves every text-bearing node in `screen` (i.e. every node whose `kind.text_key()` is
    /// `Some`) against `package` for `locale`, computing each run's origin as
    /// `node.bounds.{x,y} - run_bounds.min_{x,y}` so the rendered glyphs land inside the node's
    /// allocated bounds regardless of the run's internal bearing.
    pub fn from_screen(
        screen: &'static CompiledScreenPackage,
        package: TextPackage,
        locale: &str,
    ) -> MduxResult<Self> {
        let mut runs = Vec::new();

        for node in screen.nodes {
            let Some(text_key) = node.kind.text_key() else {
                continue;
            };

            let run = package.find_run_for_string(text_key, locale).ok_or_else(|| {
                ValidationError::new(format!(
                    "approved text package does not contain a compiled run for {text_key} in locale {locale}"
                ))
            })?;
            let run_id = run.id.clone();
            let run_bounds = package.measure_run_bounds(run)?;
            let origin_x = node
                .bounds
                .x
                .checked_sub(run_bounds.min_x)
                .ok_or_else(|| ValidationError::new("screen text origin x underflowed"))?;
            let origin_y = node
                .bounds
                .y
                .checked_sub(run_bounds.min_y)
                .ok_or_else(|| ValidationError::new("screen text origin y underflowed"))?;

            let commands = {
                let runtime = TextRuntime::<MAX_GLYPH_COMMANDS_PER_RUN>::new(&package)?;
                runtime
                    .render_run(&run_id, origin_x, origin_y)?
                    .into_iter()
                    .collect::<Vec<_>>()
            };

            runs.push(ScreenTextRun {
                node_id: node.id,
                run_id,
                origin_x,
                origin_y,
                commands,
            });
        }

        Ok(Self { package, runs })
    }

    /// The resolved run for a given node id, if that node carries text.
    pub fn find_run(&self, node_id: &str) -> Option<&ScreenTextRun> {
        self.runs.iter().find(|run| run.node_id == node_id)
    }

    /// The approved string value for `text_key` in `locale`, independent of any screen node.
    pub fn resolve_label(&self, text_key: &str, locale: &str) -> MduxResult<&str> {
        self.package
            .find_approved_string(text_key, locale)
            .map(|approved_string| approved_string.value.as_str())
            .ok_or_else(|| {
                ValidationError::new(format!(
                    "approved text package does not contain approved string {text_key} for locale {locale}"
                ))
            })
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::{
        CompiledNode, CompiledNodeKind, CriticalButtonSpec, DEFAULT_STANDARD_HELLO_WORLD_STRING_ID,
        DEFAULT_STANDARD_HELLO_WORLD_TEXT, LayoutKind, LayoutSpec, Rect, SystemEvent,
        default_standard_text_package,
    };

    const SCREEN: CompiledScreenPackage = CompiledScreenPackage {
        screen_id: "ScreenTextTest",
        layout: LayoutSpec {
            kind: LayoutKind::Vertical,
            spacing: 8,
            padding: 16,
        },
        nodes: &[CompiledNode {
            id: "greeting-label",
            bounds: Rect {
                x: 24,
                y: 40,
                width: 400,
                height: 80,
            },
            kind: CompiledNodeKind::CriticalButton(CriticalButtonSpec {
                requirement_id: "REQ-TEST-001",
                text_key: DEFAULT_STANDARD_HELLO_WORLD_STRING_ID,
                color_token: "Theme.Colors.PrimaryAction",
                on_press: SystemEvent::NoOp,
            }),
        }],
        golden_references: &[],
    };

    #[test]
    fn resolves_a_run_and_commands_for_every_text_bearing_node() {
        let package = default_standard_text_package().expect("standard package should load");
        let layout =
            ScreenTextLayout::from_screen(&SCREEN, package, "en-US").expect("layout should build");

        let run = layout
            .find_run("greeting-label")
            .expect("greeting-label should have a resolved run");

        let compiled_run = layout
            .package
            .find_run(&run.run_id)
            .expect("resolved run id should exist in the package");
        let expected_command_count = compiled_run
            .glyphs
            .iter()
            .filter(|glyph| {
                layout
                    .package
                    .find_glyph(glyph.atlas_index, glyph.glyph_id)
                    .is_some_and(|atlas_glyph| atlas_glyph.width > 0 && atlas_glyph.height > 0)
            })
            .count();

        assert_eq!(run.commands.len(), expected_command_count);
        assert!(!run.commands.is_empty());
    }

    #[test]
    fn origin_matches_node_bounds_minus_run_bounds() {
        let package = default_standard_text_package().expect("standard package should load");
        let package_for_bounds = default_standard_text_package().expect("second load");
        let layout = ScreenTextLayout::from_screen(&SCREEN, package, "en-US")
            .expect("layout should build");

        let run = layout.find_run("greeting-label").expect("run should exist");
        let compiled_run = package_for_bounds
            .find_run(&run.run_id)
            .expect("run should exist in a freshly loaded package");
        let run_bounds = package_for_bounds
            .measure_run_bounds(compiled_run)
            .expect("run bounds should be measurable");

        let node = SCREEN.find_node("greeting-label").expect("node should exist");
        assert_eq!(run.origin_x, node.bounds.x - run_bounds.min_x);
        assert_eq!(run.origin_y, node.bounds.y - run_bounds.min_y);
    }

    #[test]
    fn resolve_label_returns_approved_string_value() {
        let package = default_standard_text_package().expect("standard package should load");
        let layout = ScreenTextLayout::from_screen(&SCREEN, package, "en-US")
            .expect("layout should build");

        let label = layout
            .resolve_label(DEFAULT_STANDARD_HELLO_WORLD_STRING_ID, "en-US")
            .expect("label should resolve");

        assert_eq!(label, DEFAULT_STANDARD_HELLO_WORLD_TEXT);
    }

    #[test]
    fn errors_when_locale_has_no_compiled_run() {
        let package = default_standard_text_package().expect("standard package should load");
        let error = ScreenTextLayout::from_screen(&SCREEN, package, "fr-FR")
            .expect_err("fr-FR has no compiled run in the standard package");

        assert!(error.to_string().contains("fr-FR"));
    }
}
