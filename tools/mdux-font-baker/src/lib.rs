#![forbid(unsafe_code)]

use std::collections::{BTreeMap, BTreeSet};
use std::fs;
use std::path::{Path, PathBuf};

use fontdue::{Font, FontSettings, Metrics};
use mdux_core::{MduxResult, ValidationError, validate_non_empty};
use mdux_text_authoring::{
    RasterizedGlyph, TextCompilationInput, compile_text_package, fingerprint_font_file,
    pipeline_description,
};
use mdux_text_schema::{
    ApprovedString, CompiledGlyph, CompiledTextRun, FontAsset, NumericGlyphEntry, NumericGlyphSet,
    NumericTemplate, TextDirection, TextPackage,
};
use serde::{Deserialize, Serialize};
use sha2::{Digest, Sha256};

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct BakeSummary {
    pub package_sha256: String,
    pub atlas_sha256: String,
    pub glyph_count: usize,
    pub approved_string_count: usize,
    pub run_count: usize,
    pub numeric_template_count: usize,
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct VerificationSummary {
    pub package_sha256: String,
    pub atlas_sha256: String,
    pub package_bytes_verified: usize,
    pub report_bytes_verified: usize,
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct BakeArtifacts {
    pub package: TextPackage,
    pub package_bytes: Vec<u8>,
    pub report_bytes: Vec<u8>,
    pub summary: BakeSummary,
}

#[derive(Clone, Debug, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct BakeRecipe {
    pub toolchain_id: String,
    pub unicode_version: String,
    pub atlas_width: u16,
    #[serde(default = "default_atlas_padding")]
    pub atlas_padding: u16,
    pub font: FontRecipe,
    pub approved_strings: Vec<ApprovedStringRecipe>,
    #[serde(default)]
    pub numeric_glyph_sets: Vec<NumericGlyphSetRecipe>,
    #[serde(default)]
    pub numeric_templates: Vec<NumericTemplateRecipe>,
}

#[derive(Clone, Debug, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct FontRecipe {
    pub manifest: String,
    pub pixel_height: u16,
    pub locales: Vec<String>,
}

#[derive(Clone, Debug, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct ApprovedStringRecipe {
    pub id: String,
    pub run_id: Option<String>,
    pub locale: String,
    pub value: String,
    pub direction: RecipeTextDirection,
}

#[derive(Clone, Copy, Debug, Deserialize, Eq, PartialEq)]
pub enum RecipeTextDirection {
    #[serde(rename = "ltr", alias = "left-to-right")]
    LeftToRight,
    #[serde(rename = "rtl", alias = "right-to-left")]
    RightToLeft,
}

#[derive(Clone, Debug, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct NumericGlyphSetRecipe {
    pub id: String,
    pub locale: String,
    pub characters: String,
}

#[derive(Clone, Debug, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct NumericTemplateRecipe {
    pub id: String,
    pub locale: String,
    pub prefix_run_id: String,
    pub suffix_run_id: String,
    pub glyph_set_id: String,
    pub max_chars: u8,
    #[serde(default)]
    pub allow_negative: bool,
}

#[derive(Clone, Debug, Deserialize)]
struct FontManifest {
    schema_version: u32,
    manifest_kind: String,
    asset_family: String,
    face: FontManifestFace,
}

#[derive(Clone, Debug, Deserialize)]
struct FontManifestFace {
    family: String,
    face_index: u32,
    source_file: String,
    source_sha256: String,
    source_bytes: u64,
    intended_baseline_pixel_heights: Vec<u16>,
}

#[derive(Clone, Debug)]
struct LoadedRecipe {
    recipe: BakeRecipe,
    recipe_path: PathBuf,
    recipe_text: String,
}

#[derive(Clone, Debug)]
struct FontContext {
    font: Font,
    manifest: FontManifest,
    manifest_path: PathBuf,
    font_path: PathBuf,
    source_path: String,
    font_sha256: String,
}

#[derive(Clone, Debug)]
struct GlyphShape {
    glyph_id: u16,
    metrics: Metrics,
    advance_x: i32,
}

#[derive(Clone, Copy, Debug)]
pub struct CliInvocation<'a> {
    pub recipe_path: &'a Path,
    pub package_output_path: &'a Path,
    pub report_output_path: &'a Path,
}

pub fn bake(invocation: CliInvocation<'_>) -> MduxResult<BakeSummary> {
    let artifacts = compile_recipe(invocation.recipe_path)?;
    write_bytes(invocation.package_output_path, &artifacts.package_bytes)?;
    write_bytes(invocation.report_output_path, &artifacts.report_bytes)?;
    Ok(artifacts.summary)
}

pub fn verify(invocation: CliInvocation<'_>) -> MduxResult<VerificationSummary> {
    let artifacts = compile_recipe(invocation.recipe_path)?;
    let existing_package = fs::read(invocation.package_output_path).map_err(|error| {
        ValidationError::new(format!(
            "failed to read baked package {}: {error}",
            invocation.package_output_path.display()
        ))
    })?;
    let existing_report = fs::read(invocation.report_output_path).map_err(|error| {
        ValidationError::new(format!(
            "failed to read bake report {}: {error}",
            invocation.report_output_path.display()
        ))
    })?;

    if existing_package != artifacts.package_bytes {
        return Err(ValidationError::new(
            "baked package content does not match a fresh deterministic rebuild",
        ));
    }
    if existing_report != artifacts.report_bytes {
        return Err(ValidationError::new(
            "bake report content does not match a fresh deterministic rebuild",
        ));
    }

    Ok(VerificationSummary {
        package_sha256: artifacts.summary.package_sha256,
        atlas_sha256: artifacts.summary.atlas_sha256,
        package_bytes_verified: existing_package.len(),
        report_bytes_verified: existing_report.len(),
    })
}

pub fn compile_recipe(recipe_path: impl AsRef<Path>) -> MduxResult<BakeArtifacts> {
    let loaded_recipe = load_recipe(recipe_path.as_ref())?;
    validate_recipe(&loaded_recipe.recipe)?;
    let font_context = load_font_context(&loaded_recipe)?;

    validate_locales(&loaded_recipe.recipe, &font_context)?;

    let glyph_index_by_character = glyph_index_by_character(&font_context.font);
    let rasterized_glyphs = build_rasterized_glyphs(
        &loaded_recipe.recipe,
        &font_context,
        &glyph_index_by_character,
    )?;
    let approved_strings = build_approved_strings(&loaded_recipe.recipe);
    let runs = build_runs(
        &loaded_recipe.recipe,
        &font_context.font,
        &glyph_index_by_character,
    )?;
    let numeric_glyph_sets = build_numeric_glyph_sets(
        &loaded_recipe.recipe,
        &font_context.font,
        &glyph_index_by_character,
    )?;
    let numeric_templates = build_numeric_templates(&loaded_recipe.recipe);

    let compilation_input = TextCompilationInput {
        fonts: vec![FontAsset {
            family: font_context.manifest.asset_family.clone(),
            source_path: font_context.source_path.clone(),
            sha256: font_context.font_sha256.clone(),
            face_index: font_context.manifest.face.face_index,
            pixel_height: loaded_recipe.recipe.font.pixel_height,
            locales: loaded_recipe.recipe.font.locales.clone(),
        }],
        approved_strings,
        rasterized_glyphs,
        runs,
        numeric_glyph_sets,
        numeric_templates,
        toolchain_id: loaded_recipe.recipe.toolchain_id.clone(),
        unicode_version: loaded_recipe.recipe.unicode_version.clone(),
        build_recipe: loaded_recipe.recipe_text.clone(),
        atlas_width: loaded_recipe.recipe.atlas_width,
        atlas_padding: loaded_recipe.recipe.atlas_padding,
    };

    let package = compile_text_package(compilation_input)?;
    let atlas_sha256 = package
        .atlases
        .first()
        .map(|atlas| sha256_bytes(&atlas.pixels))
        .ok_or_else(|| ValidationError::new("compiled package did not produce an atlas"))?;
    let recipe_sha256 = sha256_text(&loaded_recipe.recipe_text);
    let package_document = PackageDocument::from(&package);
    let report_document = BakeReportDocument {
        report_kind: "mdux-font-baker-report".to_string(),
        package_sha256: package.evidence.package_sha256.clone(),
        recipe_sha256,
        atlas_sha256: atlas_sha256.clone(),
        glyph_count: package.atlas_glyphs.len(),
        approved_string_count: package.approved_strings.len(),
        run_count: package.runs.len(),
        numeric_template_count: package.numeric_templates.len(),
        font_manifest_path: normalize_separators(relative_to_workspace_or_self(
            &font_context.manifest_path,
            &loaded_recipe.recipe_path,
        )),
        font_source_path: font_context.source_path.clone(),
        font_sha256: font_context.font_sha256.clone(),
        pipeline: pipeline_description().to_string(),
    };

    let package_bytes = to_pretty_json(&package_document)?;
    let report_bytes = to_pretty_json(&report_document)?;
    let summary = BakeSummary {
        package_sha256: package.evidence.package_sha256.clone(),
        atlas_sha256,
        glyph_count: package.atlas_glyphs.len(),
        approved_string_count: package.approved_strings.len(),
        run_count: package.runs.len(),
        numeric_template_count: package.numeric_templates.len(),
    };

    Ok(BakeArtifacts {
        package,
        package_bytes,
        report_bytes,
        summary,
    })
}

fn load_recipe(recipe_path: &Path) -> MduxResult<LoadedRecipe> {
    let recipe_path = canonical_existing_path(recipe_path)?;
    let recipe_text = fs::read_to_string(&recipe_path).map_err(|error| {
        ValidationError::new(format!(
            "failed to read bake recipe {}: {error}",
            recipe_path.display()
        ))
    })?;
    let recipe = toml::from_str::<BakeRecipe>(&recipe_text).map_err(|error| {
        ValidationError::new(format!(
            "failed to parse bake recipe {}: {error}",
            recipe_path.display()
        ))
    })?;

    Ok(LoadedRecipe {
        recipe,
        recipe_path,
        recipe_text,
    })
}

fn validate_recipe(recipe: &BakeRecipe) -> MduxResult<()> {
    validate_non_empty("toolchain_id", &recipe.toolchain_id)?;
    validate_non_empty("unicode_version", &recipe.unicode_version)?;
    if recipe.atlas_width == 0 {
        return Err(ValidationError::new("atlas_width must be positive"));
    }
    if recipe.approved_strings.is_empty() {
        return Err(ValidationError::new(
            "bake recipe must define at least one approved string",
        ));
    }
    if recipe.font.pixel_height == 0 {
        return Err(ValidationError::new("font pixel_height must be positive"));
    }
    if recipe.font.locales.is_empty() {
        return Err(ValidationError::new(
            "font locales must contain at least one approved locale",
        ));
    }

    let mut string_ids = BTreeSet::new();
    let mut run_ids = BTreeSet::new();
    for approved_string in &recipe.approved_strings {
        validate_non_empty("approved string id", &approved_string.id)?;
        validate_non_empty("approved string locale", &approved_string.locale)?;
        validate_non_empty("approved string value", &approved_string.value)?;
        if !string_ids.insert(approved_string.id.clone()) {
            return Err(ValidationError::new(
                "approved string ids must be unique within a bake recipe",
            ));
        }
        let run_id = run_id_for(approved_string);
        if !run_ids.insert(run_id) {
            return Err(ValidationError::new(
                "compiled run ids must be unique within a bake recipe",
            ));
        }
    }

    let mut glyph_set_ids = BTreeSet::new();
    for glyph_set in &recipe.numeric_glyph_sets {
        validate_non_empty("numeric glyph set id", &glyph_set.id)?;
        validate_non_empty("numeric glyph set locale", &glyph_set.locale)?;
        validate_non_empty("numeric glyph set characters", &glyph_set.characters)?;
        if !glyph_set_ids.insert(glyph_set.id.clone()) {
            return Err(ValidationError::new(
                "numeric glyph set ids must be unique within a bake recipe",
            ));
        }
        let mut characters = BTreeSet::new();
        for character in glyph_set.characters.chars() {
            if !characters.insert(character) {
                return Err(ValidationError::new(format!(
                    "numeric glyph set {} contains duplicate character '{character}'",
                    glyph_set.id
                )));
            }
        }
    }

    let mut template_ids = BTreeSet::new();
    for template in &recipe.numeric_templates {
        validate_non_empty("numeric template id", &template.id)?;
        validate_non_empty("numeric template locale", &template.locale)?;
        validate_non_empty("numeric template prefix_run_id", &template.prefix_run_id)?;
        validate_non_empty("numeric template suffix_run_id", &template.suffix_run_id)?;
        validate_non_empty("numeric template glyph_set_id", &template.glyph_set_id)?;
        if template.max_chars == 0 {
            return Err(ValidationError::new(
                "numeric template max_chars must be positive",
            ));
        }
        if !template_ids.insert(template.id.clone()) {
            return Err(ValidationError::new(
                "numeric template ids must be unique within a bake recipe",
            ));
        }
    }

    Ok(())
}

fn load_font_context(loaded_recipe: &LoadedRecipe) -> MduxResult<FontContext> {
    let manifest_path = canonical_existing_path(&resolve_path(
        loaded_recipe
            .recipe_path
            .parent()
            .ok_or_else(|| ValidationError::new("bake recipe path must have a parent directory"))?,
        &loaded_recipe.recipe.font.manifest,
    ))?;
    let manifest_text = fs::read_to_string(&manifest_path).map_err(|error| {
        ValidationError::new(format!(
            "failed to read font manifest {}: {error}",
            manifest_path.display()
        ))
    })?;
    let manifest = toml::from_str::<FontManifest>(&manifest_text).map_err(|error| {
        ValidationError::new(format!(
            "failed to parse font manifest {}: {error}",
            manifest_path.display()
        ))
    })?;

    if manifest.schema_version != 1 {
        return Err(ValidationError::new(
            "font manifest schema_version must be 1",
        ));
    }
    if manifest.manifest_kind != "mdux-font-asset" {
        return Err(ValidationError::new(
            "font manifest kind must be mdux-font-asset",
        ));
    }
    if manifest.face.family != manifest.asset_family {
        return Err(ValidationError::new(
            "font manifest asset family and face family must match",
        ));
    }
    if !manifest
        .face
        .intended_baseline_pixel_heights
        .contains(&loaded_recipe.recipe.font.pixel_height)
    {
        return Err(ValidationError::new(format!(
            "font pixel_height {} is not approved by the font manifest",
            loaded_recipe.recipe.font.pixel_height
        )));
    }

    let manifest_dir = manifest_path
        .parent()
        .ok_or_else(|| ValidationError::new("font manifest path must have a parent directory"))?;
    let font_path = canonical_existing_path(&manifest_dir.join(&manifest.face.source_file))?;
    let fingerprint = fingerprint_font_file(&font_path)?;
    if fingerprint.sha256 != manifest.face.source_sha256 {
        return Err(ValidationError::new(format!(
            "font file digest mismatch for {}",
            font_path.display()
        )));
    }
    if fingerprint.byte_len != manifest.face.source_bytes {
        return Err(ValidationError::new(format!(
            "font file length mismatch for {}",
            font_path.display()
        )));
    }

    let font_bytes = fs::read(&font_path).map_err(|error| {
        ValidationError::new(format!(
            "failed to read font file {}: {error}",
            font_path.display()
        ))
    })?;
    let font = Font::from_bytes(
        font_bytes,
        FontSettings {
            collection_index: manifest.face.face_index,
            ..FontSettings::default()
        },
    )
    .map_err(|error| ValidationError::new(format!("failed to load font: {error}")))?;

    Ok(FontContext {
        font,
        manifest,
        manifest_path: manifest_path.clone(),
        font_path: font_path.clone(),
        source_path: normalize_separators(relative_to_workspace_or_self(
            &font_path,
            &loaded_recipe.recipe_path,
        )),
        font_sha256: fingerprint.sha256,
    })
}

fn validate_locales(recipe: &BakeRecipe, font_context: &FontContext) -> MduxResult<()> {
    let font_locales: BTreeSet<_> = recipe.font.locales.iter().map(String::as_str).collect();
    for approved_string in &recipe.approved_strings {
        if !font_locales.contains(approved_string.locale.as_str()) {
            return Err(ValidationError::new(format!(
                "approved string {} uses locale {} outside the font locale allow-list",
                approved_string.id, approved_string.locale
            )));
        }
        if matches!(approved_string.direction, RecipeTextDirection::RightToLeft) {
            return Err(ValidationError::new(format!(
                "approved string {} uses right-to-left text, which is not yet supported by the host baker",
                approved_string.id
            )));
        }
    }
    for glyph_set in &recipe.numeric_glyph_sets {
        if !font_locales.contains(glyph_set.locale.as_str()) {
            return Err(ValidationError::new(format!(
                "numeric glyph set {} uses locale {} outside the font locale allow-list",
                glyph_set.id, glyph_set.locale
            )));
        }
    }
    for template in &recipe.numeric_templates {
        if !font_locales.contains(template.locale.as_str()) {
            return Err(ValidationError::new(format!(
                "numeric template {} uses locale {} outside the font locale allow-list",
                template.id, template.locale
            )));
        }
    }

    let run_ids: BTreeSet<_> = recipe.approved_strings.iter().map(run_id_for).collect();
    let glyph_set_ids: BTreeSet<_> = recipe
        .numeric_glyph_sets
        .iter()
        .map(|entry| entry.id.as_str())
        .collect();
    for template in &recipe.numeric_templates {
        if !run_ids.contains(template.prefix_run_id.as_str()) {
            return Err(ValidationError::new(format!(
                "numeric template {} references unknown prefix run {}",
                template.id, template.prefix_run_id
            )));
        }
        if !run_ids.contains(template.suffix_run_id.as_str()) {
            return Err(ValidationError::new(format!(
                "numeric template {} references unknown suffix run {}",
                template.id, template.suffix_run_id
            )));
        }
        if !glyph_set_ids.contains(template.glyph_set_id.as_str()) {
            return Err(ValidationError::new(format!(
                "numeric template {} references unknown numeric glyph set {}",
                template.id, template.glyph_set_id
            )));
        }
    }

    if font_context.font_path
        != font_context
            .manifest_path
            .parent()
            .unwrap()
            .join(&font_context.manifest.face.source_file)
    {
        return Err(ValidationError::new(
            "font source resolution is inconsistent with the manifest",
        ));
    }

    Ok(())
}

fn glyph_index_by_character(font: &Font) -> BTreeMap<char, u16> {
    font.chars()
        .iter()
        .map(|(character, glyph_id)| (*character, glyph_id.get()))
        .collect()
}

fn build_rasterized_glyphs(
    recipe: &BakeRecipe,
    font_context: &FontContext,
    glyph_index_by_character: &BTreeMap<char, u16>,
) -> MduxResult<Vec<RasterizedGlyph>> {
    let mut glyph_ids = BTreeSet::new();
    for approved_string in &recipe.approved_strings {
        for character in approved_string.value.chars() {
            glyph_ids.insert(glyph_id_for_character(glyph_index_by_character, character)?);
        }
    }
    for glyph_set in &recipe.numeric_glyph_sets {
        for character in glyph_set.characters.chars() {
            glyph_ids.insert(glyph_id_for_character(glyph_index_by_character, character)?);
        }
    }

    let mut glyphs = Vec::with_capacity(glyph_ids.len());
    for glyph_id in glyph_ids {
        let (metrics, pixels) = font_context
            .font
            .rasterize_indexed(glyph_id, recipe.font.pixel_height as f32);
        glyphs.push(RasterizedGlyph {
            glyph_id: u32::from(glyph_id),
            width: to_u16(metrics.width, "glyph width")?,
            height: to_u16(metrics.height, "glyph height")?,
            bearing_x: to_i16(metrics.xmin, "glyph bearing_x")?,
            bearing_y: to_i16(metrics.ymin + metrics.height as i32, "glyph bearing_y")?,
            advance_x: round_to_i32(metrics.advance_width, "glyph advance_x")?,
            pixels,
        });
    }

    Ok(glyphs)
}

fn build_approved_strings(recipe: &BakeRecipe) -> Vec<ApprovedString> {
    recipe
        .approved_strings
        .iter()
        .map(|approved_string| ApprovedString {
            id: approved_string.id.clone(),
            locale: approved_string.locale.clone(),
            value: approved_string.value.clone(),
            direction: approved_string.direction.into(),
        })
        .collect()
}

fn build_runs(
    recipe: &BakeRecipe,
    font: &Font,
    glyph_index_by_character: &BTreeMap<char, u16>,
) -> MduxResult<Vec<CompiledTextRun>> {
    let mut runs = Vec::with_capacity(recipe.approved_strings.len());
    for approved_string in &recipe.approved_strings {
        let mut shapes = Vec::with_capacity(approved_string.value.chars().count());
        for character in approved_string.value.chars() {
            let glyph_id = glyph_id_for_character(glyph_index_by_character, character)?;
            let metrics = font.metrics_indexed(glyph_id, recipe.font.pixel_height as f32);
            let advance_x = round_to_i32(metrics.advance_width, "run advance_x")?;
            shapes.push(GlyphShape {
                glyph_id,
                metrics,
                advance_x,
            });
        }

        let mut kern_to_next = vec![0i32; shapes.len()];
        for index in 0..shapes.len().saturating_sub(1) {
            let kerning = font
                .horizontal_kern_indexed(
                    shapes[index].glyph_id,
                    shapes[index + 1].glyph_id,
                    recipe.font.pixel_height as f32,
                )
                .unwrap_or(0.0);
            kern_to_next[index] = round_to_i32(kerning, "pair kerning")?;
        }

        let mut cursor_x = 0i32;
        let mut glyphs = Vec::with_capacity(shapes.len());
        for (index, shape) in shapes.iter().enumerate() {
            let width_i32 = i32::try_from(shape.metrics.height)
                .map_err(|_| ValidationError::new("glyph height exceeds i32 range"))?;
            let advance_x = shape.advance_x + kern_to_next[index];
            glyphs.push(CompiledGlyph {
                atlas_index: 0,
                glyph_id: u32::from(shape.glyph_id),
                x: cursor_x + shape.metrics.xmin,
                y: -(shape.metrics.ymin + width_i32),
                advance_x,
            });
            cursor_x += advance_x;
        }

        runs.push(CompiledTextRun {
            id: run_id_for(approved_string),
            source_string_id: approved_string.id.clone(),
            locale: approved_string.locale.clone(),
            bidi_level: 0,
            glyphs,
        });
    }

    Ok(runs)
}

fn build_numeric_glyph_sets(
    recipe: &BakeRecipe,
    font: &Font,
    glyph_index_by_character: &BTreeMap<char, u16>,
) -> MduxResult<Vec<NumericGlyphSet>> {
    let mut glyph_sets = Vec::with_capacity(recipe.numeric_glyph_sets.len());
    for glyph_set in &recipe.numeric_glyph_sets {
        let mut entries = Vec::with_capacity(glyph_set.characters.chars().count());
        for character in glyph_set.characters.chars() {
            let glyph_id = glyph_id_for_character(glyph_index_by_character, character)?;
            let metrics = font.metrics_indexed(glyph_id, recipe.font.pixel_height as f32);
            entries.push(NumericGlyphEntry {
                character,
                glyph_id: u32::from(glyph_id),
                atlas_index: 0,
                advance_x: round_to_i32(metrics.advance_width, "numeric glyph advance_x")?,
            });
        }

        glyph_sets.push(NumericGlyphSet {
            id: glyph_set.id.clone(),
            locale: glyph_set.locale.clone(),
            entries,
        });
    }

    Ok(glyph_sets)
}

fn build_numeric_templates(recipe: &BakeRecipe) -> Vec<NumericTemplate> {
    recipe
        .numeric_templates
        .iter()
        .map(|template| NumericTemplate {
            id: template.id.clone(),
            locale: template.locale.clone(),
            prefix_run_id: template.prefix_run_id.clone(),
            suffix_run_id: template.suffix_run_id.clone(),
            glyph_set_id: template.glyph_set_id.clone(),
            max_chars: template.max_chars,
            allow_negative: template.allow_negative,
        })
        .collect()
}

fn glyph_id_for_character(
    glyph_index_by_character: &BTreeMap<char, u16>,
    character: char,
) -> MduxResult<u16> {
    glyph_index_by_character
        .get(&character)
        .copied()
        .ok_or_else(|| {
            ValidationError::new(format!(
                "font asset does not contain approved character {:?}",
                character
            ))
        })
}

fn resolve_path(base: &Path, value: &str) -> PathBuf {
    let candidate = Path::new(value);
    if candidate.is_absolute() {
        candidate.to_path_buf()
    } else {
        base.join(candidate)
    }
}

fn canonical_existing_path(path: &Path) -> MduxResult<PathBuf> {
    fs::canonicalize(path).map_err(|error| {
        ValidationError::new(format!(
            "failed to canonicalize path {}: {error}",
            path.display()
        ))
    })
}

fn relative_to_workspace_or_self(target: &Path, anchor: &Path) -> String {
    let normalized_target = fs::canonicalize(target).unwrap_or_else(|_| target.to_path_buf());
    if let Some(workspace_root) = find_workspace_root(anchor) {
        if let Ok(relative) = normalized_target.strip_prefix(&workspace_root) {
            return relative.display().to_string();
        }
    }

    normalized_target
        .file_name()
        .map(|value| value.to_string_lossy().into_owned())
        .unwrap_or_else(|| normalized_target.display().to_string())
}

fn find_workspace_root(anchor: &Path) -> Option<PathBuf> {
    let normalized_anchor = fs::canonicalize(anchor).unwrap_or_else(|_| anchor.to_path_buf());
    let start = if normalized_anchor.is_dir() {
        normalized_anchor
    } else {
        normalized_anchor.parent()?.to_path_buf()
    };

    for candidate in start.ancestors() {
        let cargo_toml = candidate.join("Cargo.toml");
        let Ok(manifest) = fs::read_to_string(&cargo_toml) else {
            continue;
        };
        if manifest.contains("[workspace]") {
            return Some(candidate.to_path_buf());
        }
    }

    None
}

fn normalize_separators(value: String) -> String {
    value.replace('\\', "/")
}

fn run_id_for(approved_string: &ApprovedStringRecipe) -> String {
    approved_string
        .run_id
        .clone()
        .unwrap_or_else(|| format!("RUN-{}", approved_string.id))
}

fn write_bytes(path: &Path, bytes: &[u8]) -> MduxResult<()> {
    if let Some(parent) = path.parent() {
        if !parent.as_os_str().is_empty() {
            fs::create_dir_all(parent).map_err(|error| {
                ValidationError::new(format!(
                    "failed to create output directory {}: {error}",
                    parent.display()
                ))
            })?;
        }
    }
    fs::write(path, bytes).map_err(|error| {
        ValidationError::new(format!("failed to write {}: {error}", path.display()))
    })
}

fn to_pretty_json<T: Serialize>(value: &T) -> MduxResult<Vec<u8>> {
    let mut bytes = serde_json::to_vec_pretty(value)
        .map_err(|error| ValidationError::new(format!("failed to serialize JSON: {error}")))?;
    bytes.push(b'\n');
    Ok(bytes)
}

fn sha256_text(value: &str) -> String {
    sha256_bytes(value.as_bytes())
}

fn sha256_bytes(bytes: &[u8]) -> String {
    let digest = Sha256::digest(bytes);
    let mut encoded = String::with_capacity(digest.len() * 2);
    for byte in digest {
        encoded.push_str(&format!("{byte:02x}"));
    }
    encoded
}

fn encode_hex(bytes: &[u8]) -> String {
    let mut encoded = String::with_capacity(bytes.len() * 2);
    for byte in bytes {
        encoded.push_str(&format!("{byte:02x}"));
    }
    encoded
}

fn default_atlas_padding() -> u16 {
    1
}

fn to_u16(value: usize, label: &str) -> MduxResult<u16> {
    u16::try_from(value).map_err(|_| ValidationError::new(format!("{label} exceeds u16 range")))
}

fn to_i16(value: i32, label: &str) -> MduxResult<i16> {
    i16::try_from(value).map_err(|_| ValidationError::new(format!("{label} exceeds i16 range")))
}

fn round_to_i32(value: f32, label: &str) -> MduxResult<i32> {
    let rounded = value.round();
    if !rounded.is_finite() || rounded < i32::MIN as f32 || rounded > i32::MAX as f32 {
        return Err(ValidationError::new(format!("{label} exceeds i32 range")));
    }
    Ok(rounded as i32)
}

impl From<RecipeTextDirection> for TextDirection {
    fn from(value: RecipeTextDirection) -> Self {
        match value {
            RecipeTextDirection::LeftToRight => TextDirection::LeftToRight,
            RecipeTextDirection::RightToLeft => TextDirection::RightToLeft,
        }
    }
}

#[derive(Clone, Debug, Serialize)]
struct BakeReportDocument {
    report_kind: String,
    package_sha256: String,
    recipe_sha256: String,
    atlas_sha256: String,
    glyph_count: usize,
    approved_string_count: usize,
    run_count: usize,
    numeric_template_count: usize,
    font_manifest_path: String,
    font_source_path: String,
    font_sha256: String,
    pipeline: String,
}

#[derive(Clone, Debug, Serialize)]
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

#[derive(Clone, Debug, Serialize)]
struct FontAssetDocument {
    family: String,
    source_path: String,
    sha256: String,
    face_index: u32,
    pixel_height: u16,
    locales: Vec<String>,
}

#[derive(Clone, Debug, Serialize)]
struct ApprovedStringDocument {
    id: String,
    locale: String,
    value: String,
    direction: &'static str,
}

#[derive(Clone, Debug, Serialize)]
struct TextureAtlasDocument {
    width: u16,
    height: u16,
    pixels_hex: String,
}

#[derive(Clone, Debug, Serialize)]
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

#[derive(Clone, Debug, Serialize)]
struct CompiledGlyphDocument {
    atlas_index: u16,
    glyph_id: u32,
    x: i32,
    y: i32,
    advance_x: i32,
}

#[derive(Clone, Debug, Serialize)]
struct CompiledTextRunDocument {
    id: String,
    source_string_id: String,
    locale: String,
    bidi_level: u8,
    glyphs: Vec<CompiledGlyphDocument>,
}

#[derive(Clone, Debug, Serialize)]
struct NumericGlyphEntryDocument {
    character: char,
    glyph_id: u32,
    atlas_index: u16,
    advance_x: i32,
}

#[derive(Clone, Debug, Serialize)]
struct NumericGlyphSetDocument {
    id: String,
    locale: String,
    entries: Vec<NumericGlyphEntryDocument>,
}

#[derive(Clone, Debug, Serialize)]
struct NumericTemplateDocument {
    id: String,
    locale: String,
    prefix_run_id: String,
    suffix_run_id: String,
    glyph_set_id: String,
    max_chars: u8,
    allow_negative: bool,
}

#[derive(Clone, Debug, Serialize)]
struct DeterminismEvidenceDocument {
    package_sha256: String,
    toolchain_id: String,
    unicode_version: String,
    build_recipe_sha256: String,
}

impl From<&TextPackage> for PackageDocument {
    fn from(package: &TextPackage) -> Self {
        Self {
            fonts: package
                .fonts
                .iter()
                .map(|font| FontAssetDocument {
                    family: font.family.clone(),
                    source_path: font.source_path.clone(),
                    sha256: font.sha256.clone(),
                    face_index: font.face_index,
                    pixel_height: font.pixel_height,
                    locales: font.locales.clone(),
                })
                .collect(),
            approved_strings: package
                .approved_strings
                .iter()
                .map(|approved_string| ApprovedStringDocument {
                    id: approved_string.id.clone(),
                    locale: approved_string.locale.clone(),
                    value: approved_string.value.clone(),
                    direction: match approved_string.direction {
                        TextDirection::LeftToRight => "ltr",
                        TextDirection::RightToLeft => "rtl",
                    },
                })
                .collect(),
            atlases: package
                .atlases
                .iter()
                .map(|atlas| TextureAtlasDocument {
                    width: atlas.width,
                    height: atlas.height,
                    pixels_hex: encode_hex(&atlas.pixels),
                })
                .collect(),
            atlas_glyphs: package
                .atlas_glyphs
                .iter()
                .map(|glyph| AtlasGlyphDocument {
                    atlas_index: glyph.atlas_index,
                    glyph_id: glyph.glyph_id,
                    x: glyph.x,
                    y: glyph.y,
                    width: glyph.width,
                    height: glyph.height,
                    bearing_x: glyph.bearing_x,
                    bearing_y: glyph.bearing_y,
                    advance_x: glyph.advance_x,
                })
                .collect(),
            runs: package
                .runs
                .iter()
                .map(|run| CompiledTextRunDocument {
                    id: run.id.clone(),
                    source_string_id: run.source_string_id.clone(),
                    locale: run.locale.clone(),
                    bidi_level: run.bidi_level,
                    glyphs: run
                        .glyphs
                        .iter()
                        .map(|glyph| CompiledGlyphDocument {
                            atlas_index: glyph.atlas_index,
                            glyph_id: glyph.glyph_id,
                            x: glyph.x,
                            y: glyph.y,
                            advance_x: glyph.advance_x,
                        })
                        .collect(),
                })
                .collect(),
            numeric_glyph_sets: package
                .numeric_glyph_sets
                .iter()
                .map(|glyph_set| NumericGlyphSetDocument {
                    id: glyph_set.id.clone(),
                    locale: glyph_set.locale.clone(),
                    entries: glyph_set
                        .entries
                        .iter()
                        .map(|entry| NumericGlyphEntryDocument {
                            character: entry.character,
                            glyph_id: entry.glyph_id,
                            atlas_index: entry.atlas_index,
                            advance_x: entry.advance_x,
                        })
                        .collect(),
                })
                .collect(),
            numeric_templates: package
                .numeric_templates
                .iter()
                .map(|template| NumericTemplateDocument {
                    id: template.id.clone(),
                    locale: template.locale.clone(),
                    prefix_run_id: template.prefix_run_id.clone(),
                    suffix_run_id: template.suffix_run_id.clone(),
                    glyph_set_id: template.glyph_set_id.clone(),
                    max_chars: template.max_chars,
                    allow_negative: template.allow_negative,
                })
                .collect(),
            evidence: DeterminismEvidenceDocument {
                package_sha256: package.evidence.package_sha256.clone(),
                toolchain_id: package.evidence.toolchain_id.clone(),
                unicode_version: package.evidence.unicode_version.clone(),
                build_recipe_sha256: package.evidence.build_recipe_sha256.clone(),
            },
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::sync::{Mutex, OnceLock};
    use std::time::{SystemTime, UNIX_EPOCH};

    fn temp_dir(prefix: &str) -> PathBuf {
        let unique = format!(
            "{}-{}-{}",
            prefix,
            std::process::id(),
            SystemTime::now()
                .duration_since(UNIX_EPOCH)
                .expect("system time should be after unix epoch")
                .as_nanos()
        );
        let path = std::env::temp_dir().join(unique);
        fs::create_dir_all(&path).expect("temporary directory should be creatable");
        path
    }

    fn fixture_recipe_path() -> PathBuf {
        PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("fixtures/roboto-demo.toml")
    }

    fn target_output_paths() -> (PathBuf, PathBuf) {
        let base =
            PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("../../target/mdux-font-baker-tests");
        (
            base.join("roboto-demo.package.json"),
            base.join("roboto-demo.report.json"),
        )
    }

    #[test]
    fn bakes_and_verifies_fixture_recipe() {
        let recipe_path = fixture_recipe_path();
        let (package_output_path, report_output_path) = target_output_paths();
        let invocation = CliInvocation {
            recipe_path: &recipe_path,
            package_output_path: &package_output_path,
            report_output_path: &report_output_path,
        };

        let summary = bake(invocation.clone()).expect("fixture recipe should bake successfully");
        assert!(summary.glyph_count > 0);
        assert!(package_output_path.exists());
        assert!(report_output_path.exists());

        let verification = verify(invocation).expect("fixture bake should verify successfully");
        assert_eq!(verification.package_sha256, summary.package_sha256);
        assert_eq!(verification.atlas_sha256, summary.atlas_sha256);

        fs::remove_file(package_output_path).expect("package output should be removable");
        fs::remove_file(report_output_path).expect("report output should be removable");
    }

    #[test]
    fn find_workspace_root_skips_missing_manifests() {
        let root = temp_dir("mdux-font-baker-workspace");
        let nested = root.join("tools/mdux-font-baker/fixtures");
        fs::create_dir_all(&nested).expect("nested directory should be creatable");
        fs::write(root.join("Cargo.toml"), "[workspace]\nmembers = []\n")
            .expect("workspace manifest should be writable");
        let recipe_path = nested.join("roboto-demo.toml");
        fs::write(&recipe_path, "toolchain_id = \"test\"\n")
            .expect("recipe file should be writable");

        let workspace_root =
            find_workspace_root(&recipe_path).expect("workspace root should resolve");

        assert_eq!(workspace_root, root);
        fs::remove_dir_all(root).expect("temporary workspace should be removable");
    }

    #[test]
    fn relative_to_workspace_or_self_normalizes_dotdot_segments() {
        let root = temp_dir("mdux-font-baker-relative");
        let font_dir = root.join("assets/fonts/roboto");
        let recipe_dir = root.join("tools/mdux-font-baker/fixtures");
        fs::create_dir_all(&font_dir).expect("font directory should be creatable");
        fs::create_dir_all(&recipe_dir).expect("recipe directory should be creatable");
        fs::write(root.join("Cargo.toml"), "[workspace]\nmembers = []\n")
            .expect("workspace manifest should be writable");
        fs::write(font_dir.join("Roboto-Regular.ttf"), b"font")
            .expect("font file should be writable");
        fs::write(
            recipe_dir.join("roboto-demo.toml"),
            "toolchain_id = \"test\"\n",
        )
        .expect("recipe file should be writable");

        let relative = relative_to_workspace_or_self(
            &font_dir.join("../roboto/Roboto-Regular.ttf"),
            &recipe_dir.join("../fixtures/roboto-demo.toml"),
        );

        assert_eq!(
            normalize_separators(relative),
            "assets/fonts/roboto/Roboto-Regular.ttf"
        );
        fs::remove_dir_all(root).expect("temporary workspace should be removable");
    }

    #[test]
    fn write_bytes_accepts_plain_filename_outputs() {
        static CURRENT_DIR_LOCK: OnceLock<Mutex<()>> = OnceLock::new();

        let guard = CURRENT_DIR_LOCK
            .get_or_init(|| Mutex::new(()))
            .lock()
            .expect("current directory lock should be acquirable");
        let root = temp_dir("mdux-font-baker-write-bytes");
        let previous_dir = std::env::current_dir().expect("current directory should be readable");
        std::env::set_current_dir(&root)
            .expect("temporary directory should become current directory");

        let write_result = write_bytes(Path::new("package.json"), b"{}");
        let written =
            fs::read(root.join("package.json")).expect("plain filename output should be written");

        std::env::set_current_dir(previous_dir)
            .expect("original current directory should be restorable");
        drop(guard);

        write_result.expect("writing a plain filename should succeed");
        assert_eq!(written, b"{}");
        fs::remove_dir_all(root).expect("temporary workspace should be removable");
    }
}
