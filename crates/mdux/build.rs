use std::env;
use std::fmt::{self, Write as _};
use std::fs;
use std::path::{Path, PathBuf};

use serde::Deserialize;

const DEFAULT_STANDARD_PACKAGE_JSON: &str = "../../generated/fonts/roboto-regular-16px/package.json";

fn main() -> Result<(), Box<dyn std::error::Error>> {
    let manifest_dir = PathBuf::from(env::var("CARGO_MANIFEST_DIR")?);
    let package_path = manifest_dir.join(DEFAULT_STANDARD_PACKAGE_JSON);
    println!("cargo:rerun-if-changed={}", package_path.display());

    let document_text = fs::read_to_string(&package_path)?;
    let package_document: PackageDocument = serde_json::from_str(&document_text)?;
    let rendered = render_text_package(&package_document, &package_path)?;
    fs::write(
        PathBuf::from(env::var("OUT_DIR")?).join("default_standard_text_package.rs"),
        rendered,
    )?;

    Ok(())
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

#[derive(Clone, Debug, Deserialize)]
#[serde(deny_unknown_fields)]
struct ApprovedStringDocument {
    id: String,
    locale: String,
    value: String,
    direction: TextDirectionDocument,
}

#[derive(Clone, Copy, Debug, Deserialize)]
enum TextDirectionDocument {
    #[serde(rename = "ltr")]
    LeftToRight,
    #[serde(rename = "rtl")]
    RightToLeft,
}

#[derive(Clone, Debug, Deserialize)]
#[serde(deny_unknown_fields)]
struct TextureAtlasDocument {
    width: u16,
    height: u16,
    pixels_hex: String,
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

#[derive(Clone, Debug, Deserialize)]
#[serde(deny_unknown_fields)]
struct CompiledGlyphDocument {
    atlas_index: u16,
    glyph_id: u32,
    x: i32,
    y: i32,
    advance_x: i32,
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

#[derive(Clone, Debug, Deserialize)]
#[serde(deny_unknown_fields)]
struct NumericGlyphEntryDocument {
    character: char,
    glyph_id: u32,
    atlas_index: u16,
    advance_x: i32,
}

#[derive(Clone, Debug, Deserialize)]
#[serde(deny_unknown_fields)]
struct NumericGlyphSetDocument {
    id: String,
    locale: String,
    entries: Vec<NumericGlyphEntryDocument>,
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

#[derive(Clone, Debug, Deserialize)]
#[serde(deny_unknown_fields)]
struct DeterminismEvidenceDocument {
    package_sha256: String,
    toolchain_id: String,
    unicode_version: String,
    build_recipe_sha256: String,
}

fn render_text_package(document: &PackageDocument, package_path: &Path) -> Result<String, String> {
    let mut rendered = String::new();
    rendered.push_str("fn build_default_standard_text_package() -> TextPackage {\n");
    rendered.push_str("    TextPackage {\n");
    rendered.push_str("        fonts: vec![\n");
    for font in &document.fonts {
        writeln!(
            rendered,
            "            FontAsset {{ family: {}, source_path: {}, sha256: {}, face_index: {}, pixel_height: {}, locales: vec![{}] }},",
            rust_string(&font.family),
            rust_string(&font.source_path),
            rust_string(&font.sha256),
            font.face_index,
            font.pixel_height,
            render_string_vec(&font.locales),
        )
        .map_err(render_fmt_error)?;
    }
    rendered.push_str("        ],\n");
    rendered.push_str("        approved_strings: vec![\n");
    for approved_string in &document.approved_strings {
        writeln!(
            rendered,
            "            ApprovedString {{ id: {}, locale: {}, value: {}, direction: {} }},",
            rust_string(&approved_string.id),
            rust_string(&approved_string.locale),
            rust_string(&approved_string.value),
            approved_string.direction.render(),
        )
        .map_err(render_fmt_error)?;
    }
    rendered.push_str("        ],\n");
    rendered.push_str("        atlases: vec![\n");
    for (atlas_index, atlas) in document.atlases.iter().enumerate() {
        writeln!(
            rendered,
            "            TextureAtlas {{ width: {}, height: {}, pixels: vec![{}] }},",
            atlas.width,
            atlas.height,
            render_u8_vec(&decode_hex(package_path, atlas_index, &atlas.pixels_hex)?),
        )
        .map_err(render_fmt_error)?;
    }
    rendered.push_str("        ],\n");
    rendered.push_str("        atlas_glyphs: vec![\n");
    for glyph in &document.atlas_glyphs {
        writeln!(
            rendered,
            "            AtlasGlyph {{ atlas_index: {}, glyph_id: {}, x: {}, y: {}, width: {}, height: {}, bearing_x: {}, bearing_y: {}, advance_x: {} }},",
            glyph.atlas_index,
            glyph.glyph_id,
            glyph.x,
            glyph.y,
            glyph.width,
            glyph.height,
            glyph.bearing_x,
            glyph.bearing_y,
            glyph.advance_x,
        )
        .map_err(render_fmt_error)?;
    }
    rendered.push_str("        ],\n");
    rendered.push_str("        runs: vec![\n");
    for run in &document.runs {
        writeln!(
            rendered,
            "            CompiledTextRun {{ id: {}, source_string_id: {}, locale: {}, bidi_level: {}, glyphs: vec![{}] }},",
            rust_string(&run.id),
            rust_string(&run.source_string_id),
            rust_string(&run.locale),
            run.bidi_level,
            run.glyphs
                .iter()
                .map(render_compiled_glyph)
                .collect::<Vec<_>>()
                .join(", "),
        )
        .map_err(render_fmt_error)?;
    }
    rendered.push_str("        ],\n");
    rendered.push_str("        numeric_glyph_sets: vec![\n");
    for glyph_set in &document.numeric_glyph_sets {
        writeln!(
            rendered,
            "            NumericGlyphSet {{ id: {}, locale: {}, entries: vec![{}] }},",
            rust_string(&glyph_set.id),
            rust_string(&glyph_set.locale),
            glyph_set
                .entries
                .iter()
                .map(render_numeric_glyph_entry)
                .collect::<Vec<_>>()
                .join(", "),
        )
        .map_err(render_fmt_error)?;
    }
    rendered.push_str("        ],\n");
    rendered.push_str("        numeric_templates: vec![\n");
    for template in &document.numeric_templates {
        writeln!(
            rendered,
            "            NumericTemplate {{ id: {}, locale: {}, prefix_run_id: {}, suffix_run_id: {}, glyph_set_id: {}, max_chars: {}, allow_negative: {} }},",
            rust_string(&template.id),
            rust_string(&template.locale),
            rust_string(&template.prefix_run_id),
            rust_string(&template.suffix_run_id),
            rust_string(&template.glyph_set_id),
            template.max_chars,
            template.allow_negative,
        )
        .map_err(render_fmt_error)?;
    }
    rendered.push_str("        ],\n");
    writeln!(
        rendered,
        "        evidence: DeterminismEvidence {{ package_sha256: {}, toolchain_id: {}, unicode_version: {}, build_recipe_sha256: {} }},",
        rust_string(&document.evidence.package_sha256),
        rust_string(&document.evidence.toolchain_id),
        rust_string(&document.evidence.unicode_version),
        rust_string(&document.evidence.build_recipe_sha256),
    )
    .map_err(render_fmt_error)?;
    rendered.push_str("    }\n");
    rendered.push_str("}\n");

    Ok(rendered)
}

fn render_string_vec(values: &[String]) -> String {
    values
        .iter()
        .map(|value| format!("{value:?}.to_string()"))
        .collect::<Vec<_>>()
        .join(", ")
}

fn render_u8_vec(values: &[u8]) -> String {
    values
        .iter()
        .map(|value| format!("{value}u8"))
        .collect::<Vec<_>>()
        .join(", ")
}

fn render_compiled_glyph(glyph: &CompiledGlyphDocument) -> String {
    format!(
        "CompiledGlyph {{ atlas_index: {}, glyph_id: {}, x: {}, y: {}, advance_x: {} }}",
        glyph.atlas_index, glyph.glyph_id, glyph.x, glyph.y, glyph.advance_x
    )
}

fn render_numeric_glyph_entry(entry: &NumericGlyphEntryDocument) -> String {
    format!(
        "NumericGlyphEntry {{ character: {}, glyph_id: {}, atlas_index: {}, advance_x: {} }}",
        rust_char(entry.character),
        entry.glyph_id,
        entry.atlas_index,
        entry.advance_x,
    )
}

fn rust_string(value: &str) -> String {
    format!("{value:?}.to_string()")
}

fn rust_char(value: char) -> String {
    format!("{value:?}")
}

fn decode_hex(package_path: &Path, atlas_index: usize, encoded: &str) -> Result<Vec<u8>, String> {
    let bytes = encoded.as_bytes();
    if bytes.len() % 2 != 0 {
        return Err(format!(
            "text package {} atlas {} pixels_hex must have an even number of characters",
            package_path.display(),
            atlas_index
        ));
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
    package_path: &Path,
    atlas_index: usize,
    byte: u8,
    offset: usize,
) -> Result<u8, String> {
    match byte {
        b'0'..=b'9' => Ok(byte - b'0'),
        b'a'..=b'f' => Ok(byte - b'a' + 10),
        b'A'..=b'F' => Ok(byte - b'A' + 10),
        _ => Err(format!(
            "text package {} atlas {} pixels_hex contains invalid hex at byte {}",
            package_path.display(),
            atlas_index,
            offset
        )),
    }
}

fn render_fmt_error(error: fmt::Error) -> String {
    format!("failed to render generated standard text source: {error}")
}

impl TextDirectionDocument {
    fn render(self) -> &'static str {
        match self {
            Self::LeftToRight => "TextDirection::LeftToRight",
            Self::RightToLeft => "TextDirection::RightToLeft",
        }
    }
}
