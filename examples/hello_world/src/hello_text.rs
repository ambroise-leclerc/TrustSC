use mdux::{
    default_standard_text_package, GlyphDrawCommand, MduxResult, TextPackage, TextRuntime,
    ValidationError, DEFAULT_STANDARD_HELLO_WORLD_RUN_ID,
};
#[cfg(test)]
use mdux::{DEFAULT_STANDARD_HELLO_WORLD_STRING_ID, DEFAULT_STANDARD_HELLO_WORLD_TEXT};

#[cfg(test)]
pub const HELLO_WORLD_TEXT: &str = DEFAULT_STANDARD_HELLO_WORLD_TEXT;
#[cfg(test)]
pub const HELLO_WORLD_STRING_ID: &str = DEFAULT_STANDARD_HELLO_WORLD_STRING_ID;
pub const HELLO_WORLD_RUN_ID: &str = DEFAULT_STANDARD_HELLO_WORLD_RUN_ID;
pub const HELLO_WORLD_DRAW_COMMAND_COUNT: usize = 11;

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct HelloWorldTextLayout {
    pub package: TextPackage,
    pub commands: [GlyphDrawCommand; HELLO_WORLD_DRAW_COMMAND_COUNT],
}

pub fn hello_world_text_package() -> MduxResult<TextPackage> {
    default_standard_text_package()
}

#[allow(dead_code)]
pub fn hello_world_glyph_draw_commands(
    origin_x: i32,
    origin_y: i32,
) -> MduxResult<[GlyphDrawCommand; HELLO_WORLD_DRAW_COMMAND_COUNT]> {
    Ok(hello_world_text_layout(origin_x, origin_y)?.commands)
}

pub fn hello_world_text_layout(origin_x: i32, origin_y: i32) -> MduxResult<HelloWorldTextLayout> {
    let package = hello_world_text_package()?;
    let commands = {
        let runtime = TextRuntime::<HELLO_WORLD_DRAW_COMMAND_COUNT>::new(&package)?;
        runtime
            .render_run(HELLO_WORLD_RUN_ID, origin_x, origin_y)?
            .into_inner()
            .map_err(|_| ValidationError::new("hello world command count changed unexpectedly"))?
    };

    Ok(HelloWorldTextLayout { package, commands })
}

#[cfg(test)]
mod tests {
    use super::*;
    use mdux::DEFAULT_STANDARD_FONT;

    #[test]
    fn loads_default_standard_roboto_package() {
        let first = hello_world_text_package().expect("first package should load");
        let second = hello_world_text_package().expect("second package should load");
        let approved_string = first
            .approved_strings
            .iter()
            .find(|approved_string| approved_string.id == HELLO_WORLD_STRING_ID)
            .expect("hello world string should exist");
        let run = first
            .find_run(HELLO_WORLD_RUN_ID)
            .expect("hello world run should exist");

        assert_eq!(first.fonts.len(), 1);
        assert_eq!(first.fonts[0].family, DEFAULT_STANDARD_FONT.family);
        assert_eq!(first.fonts[0].pixel_height, DEFAULT_STANDARD_FONT.pixel_height);
        assert_eq!(first.fonts[0].source_path, "Roboto-Regular.ttf");
        assert_eq!(approved_string.value, HELLO_WORLD_TEXT);
        assert_eq!(run.source_string_id, HELLO_WORLD_STRING_ID);
        assert_eq!(run.glyphs.len(), HELLO_WORLD_DRAW_COMMAND_COUNT + 1);
        assert_eq!(first.evidence.package_sha256, second.evidence.package_sha256);
        assert_eq!(first.atlases[0].pixels, second.atlases[0].pixels);
    }

    #[test]
    fn emits_draw_commands_for_generated_roboto_glyphs() {
        let origin_x = 10;
        let origin_y = 20;
        let layout = hello_world_text_layout(origin_x, origin_y).expect("layout should compile and render");
        let run = layout
            .package
            .find_run(HELLO_WORLD_RUN_ID)
            .expect("hello world run should exist");
        let expected_commands = run
            .glyphs
            .iter()
            .filter_map(|glyph| {
                let atlas_glyph = layout
                    .package
                    .find_glyph(glyph.atlas_index, glyph.glyph_id)
                    .expect("compiled glyph should resolve to an atlas glyph");
                (atlas_glyph.width > 0 && atlas_glyph.height > 0).then_some((
                    glyph.glyph_id,
                    origin_x + glyph.x,
                    origin_y + glyph.y,
                    atlas_glyph.width,
                    atlas_glyph.height,
                ))
            })
            .collect::<Vec<_>>();

        assert_eq!(layout.commands.len(), HELLO_WORLD_DRAW_COMMAND_COUNT);
        assert_eq!(layout.commands.len(), expected_commands.len());

        for (command, (glyph_id, x, y, width, height)) in
            layout.commands.iter().zip(expected_commands.iter().copied())
        {
            assert_eq!(command.glyph_id, glyph_id);
            assert_eq!(command.x, x);
            assert_eq!(command.y, y);
            assert_eq!(command.width, width);
            assert_eq!(command.height, height);
        }
    }

    #[test]
    fn convenience_command_helper_matches_layout_commands() {
        let layout = hello_world_text_layout(0, 0).expect("layout should render");
        let commands = hello_world_glyph_draw_commands(0, 0).expect("command helper should render");
        let run = layout
            .package
            .find_run(HELLO_WORLD_RUN_ID)
            .expect("hello world run should exist");

        assert_eq!(commands, layout.commands);
        assert_eq!(run.glyphs.len(), HELLO_WORLD_DRAW_COMMAND_COUNT + 1);
    }
}
