#![forbid(unsafe_code)]

use std::{
    fmt::Write as _,
    fs,
    path::Path,
};

use mdux_core::{MduxResult, ValidationError};
use mdux_text_schema::{CompiledTextRun, NumericGlyphSet, TextPackage};
use mdux_ui::{ClockFormat, CvCheckKind, LayoutKind, SystemEvent, THEME_COLORS, resolve_color_token};

/// Glyph set the `Clock` widget renders from (digits, `-`, `:`, space in the standard package).
/// Kept in sync with `mdux::DEFAULT_STANDARD_DIGITS_GLYPH_SET_ID`.
pub const CLOCK_GLYPH_SET_ID: &str = "SET-ASCII-DIGITS";

/// The approved text packages a screen compiles against: every static label budget checks the
/// `standard` package (all locales), while `NumericDisplay` templates resolve in the `display`
/// package (ADR-013 two-package strategy). `display` may be absent for screens without numeric
/// displays.
#[derive(Clone, Copy)]
pub struct TextPackages<'a> {
    pub standard: &'a TextPackage,
    pub display: Option<&'a TextPackage>,
}

impl<'a> TextPackages<'a> {
    pub fn standard_only(standard: &'a TextPackage) -> Self {
        Self {
            standard,
            display: None,
        }
    }

    pub fn with_display(standard: &'a TextPackage, display: &'a TextPackage) -> Self {
        Self {
            standard,
            display: Some(display),
        }
    }
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct CompileOptions {
    pub surface_width: u32,
    pub surface_height: u32,
    pub crate_path: &'static str,
}

impl CompileOptions {
    /// Generated code qualifies every type against `::mdux` by default, so the including file
    /// needs no `use` statements (see `mdux::include_medui_screen!`). Call `with_crate_path` to
    /// target a different root, e.g. in-crate tests that re-export the same types locally.
    pub const fn new(surface_width: u32, surface_height: u32) -> Self {
        Self {
            surface_width,
            surface_height,
            crate_path: "::mdux",
        }
    }

    pub const fn with_crate_path(mut self, crate_path: &'static str) -> Self {
        self.crate_path = crate_path;
        self
    }
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
enum Dimension {
    Px(u32),
    Fill,
}

#[derive(Clone, Debug, Eq, PartialEq)]
struct LayoutDefinition {
    kind: LayoutKind,
    spacing: u16,
    padding: u16,
}

#[derive(Clone, Debug, Eq, PartialEq)]
struct SafetyCriticalDefinition {
    cv_checks: Vec<CvCheckKind>,
}

#[derive(Clone, Debug, Eq, PartialEq)]
struct ScreenDefinition {
    id: String,
    layout: LayoutDefinition,
    /// Optional `surface: WxH` pin (ADR-014): compile fails if it disagrees with the build's
    /// configured surface.
    declared_surface: Option<(u32, u32)>,
    items: Vec<ScreenItem>,
}

/// A top-level entry in the screen flow: either a leaf component, or a `Row` container laying
/// its children out horizontally. Rows exist at compile time only — the emitted package is flat.
#[derive(Clone, Debug, Eq, PartialEq)]
enum ScreenItem {
    Component(NodeDefinition),
    Row(RowDefinition),
}

#[derive(Clone, Debug, Eq, PartialEq)]
struct RowDefinition {
    id: String,
    height: Dimension,
    spacing: u16,
    /// Optional background color token: emits a synthetic Panel node spanning the row.
    background: Option<String>,
    children: Vec<NodeDefinition>,
}

#[derive(Clone, Debug, Eq, PartialEq)]
struct NodeDefinition {
    id: String,
    width: Dimension,
    height: Dimension,
    /// ADR-014 absolute placement: screen coordinates of the top-left corner, out of flow.
    position: Option<(u32, u32)>,
    kind: NodeKind,
    safety_critical: Option<SafetyCriticalDefinition>,
}

#[derive(Clone, Debug, Eq, PartialEq)]
enum NodeKind {
    CriticalButton {
        requirement_id: String,
        label_text_key: String,
        color_token: String,
        on_press: SystemEvent,
    },
    VulkanViewport {
        stream_source: String,
    },
    Label {
        text_key: String,
        color_token: String,
    },
    Clock {
        format: ClockFormat,
    },
    NumericDisplay {
        requirement_id: String,
        template_id: String,
        source: String,
        color_token: String,
    },
    StatusIndicator {
        requirement_id: String,
        source: String,
        state_text_keys: Vec<String>,
        color_tokens: Vec<String>,
    },
    /// Synthetic background rectangle (Row `background:`); never parsed as a component.
    Panel {
        color_token: String,
    },
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
struct RectSpec {
    x: i32,
    y: i32,
    width: u32,
    height: u32,
}

#[derive(Clone, Debug, Eq, PartialEq)]
struct CompiledNodeSpec {
    id: String,
    bounds: RectSpec,
    kind: NodeKind,
}

#[derive(Clone, Debug, Eq, PartialEq)]
struct GoldenReferenceSpec {
    node_id: String,
    bounds: RectSpec,
    text_key: Option<String>,
    color_token: Option<String>,
    cv_checks: Vec<CvCheckKind>,
}

#[derive(Clone, Debug, Eq, PartialEq)]
struct CompiledScreenSpec {
    id: String,
    layout: LayoutDefinition,
    surface: (u32, u32),
    nodes: Vec<CompiledNodeSpec>,
    golden_references: Vec<GoldenReferenceSpec>,
}

pub fn compile_medui_file_to_rust_module(
    input_path: impl AsRef<Path>,
    output_path: impl AsRef<Path>,
    options: CompileOptions,
    text_packages: TextPackages<'_>,
) -> MduxResult<()> {
    let input_path = input_path.as_ref();
    let output_path = output_path.as_ref();
    let source = fs::read_to_string(input_path).map_err(|error| {
        ValidationError::new(format!(
            "failed to read MedUI source {}: {error}",
            input_path.display()
        ))
    })?;
    let generated = compile_medui_source_to_rust(&source, options, text_packages)?;

    if let Some(parent) = output_path.parent() {
        fs::create_dir_all(parent).map_err(|error| {
            ValidationError::new(format!(
                "failed to create MedUI output directory {}: {error}",
                parent.display()
            ))
        })?;
    }
    fs::write(output_path, generated).map_err(|error| {
        ValidationError::new(format!(
            "failed to write generated MedUI module {}: {error}",
            output_path.display()
        ))
    })?;

    Ok(())
}

pub fn compile_medui_source_to_rust(
    source: &str,
    options: CompileOptions,
    text_packages: TextPackages<'_>,
) -> MduxResult<String> {
    let parsed = parse_screen(source)?;
    let compiled = compile_screen(parsed, options, text_packages)?;
    Ok(emit_rust_module(&compiled, options.crate_path))
}

fn parse_screen(source: &str) -> MduxResult<ScreenDefinition> {
    let lines = source
        .lines()
        .enumerate()
        .filter_map(|(index, line)| {
            let trimmed = line.trim();
            (!trimmed.is_empty() && !trimmed.starts_with("//"))
                .then_some((index + 1, trimmed.to_string()))
        })
        .collect::<Vec<_>>();

    if lines.len() < 3 {
        return Err(ValidationError::new(
            "MedUI source must contain a screen header, layout, and closing brace",
        ));
    }

    let (screen_line, screen_header) = &lines[0];
    let screen_id = parse_screen_header(*screen_line, screen_header)?;
    let (layout_line, layout_header) = &lines[1];
    let layout = parse_layout(*layout_line, layout_header)?;
    let mut items = Vec::new();
    let mut pending_safety: Option<SafetyCriticalDefinition> = None;
    let mut cursor = 2usize;

    // Optional `surface: <W>px, <H>px;` pin immediately after the layout line (ADR-014).
    let mut declared_surface = None;
    if cursor < lines.len() {
        let (surface_line_number, surface_line) = &lines[cursor];
        if let Some(value) = surface_line.strip_prefix("surface:") {
            declared_surface = Some(parse_surface(
                *surface_line_number,
                value.trim().trim_end_matches(';'),
            )?);
            cursor += 1;
        }
    }

    while cursor < lines.len() {
        let (line_number, line) = &lines[cursor];
        if line == "}" {
            if cursor != lines.len() - 1 {
                return Err(ValidationError::new(format!(
                    "unexpected content after screen closing brace at line {line_number}"
                )));
            }
            break;
        }

        if line.starts_with("@safety_critical(") {
            pending_safety = Some(parse_safety_critical(*line_number, line)?);
            cursor += 1;
            continue;
        }

        if line == "Row {" {
            if pending_safety.is_some() {
                return Err(ValidationError::new(format!(
                    "@safety_critical cannot annotate a Row container at line {line_number}"
                )));
            }
            let (row, next_cursor) = parse_row(&lines, cursor)?;
            items.push(ScreenItem::Row(row));
            cursor = next_cursor;
            continue;
        }

        let component_kind = parse_component_start(*line_number, line)?;
        cursor += 1;

        let mut properties = Vec::new();
        while cursor < lines.len() {
            let (property_line_number, property_line) = &lines[cursor];
            if property_line == "}" {
                cursor += 1;
                break;
            }
            properties.push((*property_line_number, property_line.clone()));
            cursor += 1;
        }

        let node = parse_component_properties(
            *line_number,
            component_kind,
            pending_safety.take(),
            &properties,
        )?;
        items.push(ScreenItem::Component(node));
    }

    if items.is_empty() {
        return Err(ValidationError::new(
            "MedUI screen must declare at least one component",
        ));
    }

    Ok(ScreenDefinition {
        id: screen_id,
        layout,
        declared_surface,
        items,
    })
}

/// Parses a `Row { … }` container starting at `lines[start]` (which is the `Row {` line).
/// Returns the row and the cursor position after its closing brace. Rows accept scalar
/// properties (`id`, `height`, `spacing`) and nested leaf components; nesting another `Row`
/// is rejected.
fn parse_row(
    lines: &[(usize, String)],
    start: usize,
) -> MduxResult<(RowDefinition, usize)> {
    let (row_line_number, _) = &lines[start];
    let row_line_number = *row_line_number;
    let mut cursor = start + 1;

    let mut id = None;
    let mut height = None;
    let mut spacing = None;
    let mut background = None;
    let mut children = Vec::new();
    let mut pending_safety: Option<SafetyCriticalDefinition> = None;
    let mut closed = false;

    while cursor < lines.len() {
        let (line_number, line) = &lines[cursor];

        if line == "}" {
            cursor += 1;
            closed = true;
            break;
        }

        if line == "Row {" {
            return Err(ValidationError::new(format!(
                "nested Row containers are not supported at line {line_number}"
            )));
        }

        if line.starts_with("@safety_critical(") {
            pending_safety = Some(parse_safety_critical(*line_number, line)?);
            cursor += 1;
            continue;
        }

        if line.ends_with('{') {
            let component_kind = parse_component_start(*line_number, line)?;
            let component_line_number = *line_number;
            cursor += 1;

            let mut properties = Vec::new();
            while cursor < lines.len() {
                let (property_line_number, property_line) = &lines[cursor];
                if property_line == "}" {
                    cursor += 1;
                    break;
                }
                properties.push((*property_line_number, property_line.clone()));
                cursor += 1;
            }

            let node = parse_component_properties(
                component_line_number,
                component_kind,
                pending_safety.take(),
                &properties,
            )?;
            children.push(node);
            continue;
        }

        // Scalar Row property.
        let property_line = line.trim_end_matches(';');
        let (key, value) = property_line.split_once(':').ok_or_else(|| {
            ValidationError::new(format!(
                "invalid Row property `{property_line}` at line {line_number}"
            ))
        })?;
        match key.trim() {
            "id" => id = Some(parse_identifier(*line_number, "Row id", value.trim())?),
            "height" => height = Some(parse_dimension(*line_number, "height", value.trim())?),
            "spacing" => spacing = Some(parse_px(*line_number, "spacing", value.trim())?),
            "background" => {
                background = Some(parse_non_empty(*line_number, "background", value.trim())?)
            }
            other => {
                return Err(ValidationError::new(format!(
                    "unsupported Row property `{other}` at line {line_number}"
                )));
            }
        }
        cursor += 1;
    }

    if !closed {
        return Err(ValidationError::new(format!(
            "Row starting at line {row_line_number} is missing its closing brace"
        )));
    }

    let id = id.ok_or_else(|| {
        ValidationError::new(format!("Row at line {row_line_number} must declare `id`"))
    })?;
    let height = height.ok_or_else(|| {
        ValidationError::new(format!("Row {id} must declare `height`"))
    })?;
    if children.is_empty() {
        return Err(ValidationError::new(format!(
            "Row {id} must contain at least one component"
        )));
    }

    Ok((
        RowDefinition {
            id,
            height,
            spacing: spacing.unwrap_or(0),
            background,
            children,
        },
        cursor,
    ))
}

fn parse_screen_header(line_number: usize, line: &str) -> MduxResult<String> {
    let header = line
        .strip_prefix("Screen ")
        .and_then(|rest| rest.strip_suffix('{'))
        .map(str::trim)
        .ok_or_else(|| {
            ValidationError::new(format!(
                "expected `Screen <Name> {{` at line {line_number}"
            ))
        })?;
    parse_identifier(line_number, "screen id", header)
}

fn parse_layout(line_number: usize, line: &str) -> MduxResult<LayoutDefinition> {
    let payload = line
        .strip_prefix("layout:")
        .map(str::trim)
        .ok_or_else(|| ValidationError::new(format!("expected layout declaration at line {line_number}")))?;
    let (kind_name, block) = payload.split_once('{').ok_or_else(|| {
        ValidationError::new(format!(
            "layout declaration must contain an inline block at line {line_number}"
        ))
    })?;
    let layout_kind = match kind_name.trim() {
        "Vertical" => LayoutKind::Vertical,
        "Horizontal" => LayoutKind::Horizontal,
        other => {
            return Err(ValidationError::new(format!(
                "unsupported layout `{other}` at line {line_number}"
            )))
        }
    };
    let block = block.trim().trim_end_matches('}').trim();
    let spacing = parse_inline_px_property(line_number, block, "spacing")?;
    let padding = parse_inline_px_property(line_number, block, "padding")?;

    Ok(LayoutDefinition {
        kind: layout_kind,
        spacing,
        padding,
    })
}

fn parse_inline_px_property(line_number: usize, block: &str, key: &str) -> MduxResult<u16> {
    block
        .split(';')
        .map(str::trim)
        .find(|entry| entry.starts_with(&format!("{key}:")))
        .ok_or_else(|| {
            ValidationError::new(format!(
                "layout block at line {line_number} must declare `{key}`"
            ))
        })
        .and_then(|entry| {
            let (_, value) = entry.split_once(':').ok_or_else(|| {
                ValidationError::new(format!("invalid layout property `{entry}` at line {line_number}"))
            })?;
            parse_px_allowing_zero(line_number, key, value.trim())
        })
}

fn parse_safety_critical(line_number: usize, line: &str) -> MduxResult<SafetyCriticalDefinition> {
    let checks_block = line
        .split_once('[')
        .and_then(|(_, rest)| rest.split_once(']'))
        .map(|(checks, _)| checks)
        .ok_or_else(|| {
            ValidationError::new(format!(
                "expected `@safety_critical(cv_check: [...])` at line {line_number}"
            ))
        })?;
    let cv_checks = checks_block
        .split(',')
        .map(str::trim)
        .map(|entry| match entry {
            "Bounds" => Ok(CvCheckKind::Bounds),
            "ColorHash" => Ok(CvCheckKind::ColorHash),
            other => Err(ValidationError::new(format!(
                "unsupported CV check `{other}` at line {line_number}"
            ))),
        })
        .collect::<MduxResult<Vec<_>>>()?;

    if cv_checks.is_empty() {
        return Err(ValidationError::new(format!(
            "safety-critical annotation at line {line_number} must declare at least one CV check"
        )));
    }

    Ok(SafetyCriticalDefinition { cv_checks })
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
enum ComponentKind {
    CriticalButton,
    VulkanViewport,
    Label,
    Clock,
    NumericDisplay,
    StatusIndicator,
}

fn parse_component_start(line_number: usize, line: &str) -> MduxResult<ComponentKind> {
    let kind = line
        .strip_suffix('{')
        .map(str::trim)
        .ok_or_else(|| ValidationError::new(format!("expected component block at line {line_number}")))?;
    match kind {
        "CriticalButton" => Ok(ComponentKind::CriticalButton),
        "VulkanViewport" => Ok(ComponentKind::VulkanViewport),
        "Label" => Ok(ComponentKind::Label),
        "Clock" => Ok(ComponentKind::Clock),
        "NumericDisplay" => Ok(ComponentKind::NumericDisplay),
        "StatusIndicator" => Ok(ComponentKind::StatusIndicator),
        other => Err(ValidationError::new(format!(
            "unsupported component `{other}` at line {line_number}"
        ))),
    }
}

fn parse_component_properties(
    line_number: usize,
    component_kind: ComponentKind,
    safety_critical: Option<SafetyCriticalDefinition>,
    properties: &[(usize, String)],
) -> MduxResult<NodeDefinition> {
    let mut id = None;
    let mut width = None;
    let mut height = None;
    let mut requirement_id = None;
    let mut label_text_key = None;
    let mut color_token = None;
    let mut on_press = None;
    let mut stream_source = None;
    let mut text_key = None;
    let mut clock_format = None;
    let mut template_id = None;
    let mut source = None;
    let mut state_text_keys = None;
    let mut color_tokens = None;
    let mut position = None;

    for (property_line_number, property_line) in properties {
        let property_line = property_line.trim_end_matches(';');
        let (key, value) = property_line.split_once(':').ok_or_else(|| {
            ValidationError::new(format!(
                "invalid property `{property_line}` at line {property_line_number}"
            ))
        })?;
        let key = key.trim();
        let value = value.trim();
        match key {
            "id" => id = Some(parse_identifier(*property_line_number, "component id", value)?),
            "width" => width = Some(parse_dimension(*property_line_number, "width", value)?),
            "height" => height = Some(parse_dimension(*property_line_number, "height", value)?),
            "requirement" => {
                requirement_id = Some(parse_quoted(*property_line_number, "requirement", value)?)
            }
            "label" => {
                label_text_key = Some(parse_text_key(*property_line_number, value)?)
            }
            "text" => text_key = Some(parse_text_key(*property_line_number, value)?),
            "color" => color_token = Some(parse_non_empty(*property_line_number, "color", value)?),
            "on_press" => on_press = Some(parse_system_event(*property_line_number, value)?),
            "stream_source" => {
                stream_source = Some(parse_quoted(*property_line_number, "stream_source", value)?)
            }
            "format" => clock_format = Some(parse_clock_format(*property_line_number, value)?),
            "template" => {
                template_id = Some(parse_quoted(*property_line_number, "template", value)?)
            }
            "source" => source = Some(parse_quoted(*property_line_number, "source", value)?),
            "states" => {
                state_text_keys = Some(parse_text_key_list(*property_line_number, value)?)
            }
            "colors" => {
                color_tokens = Some(parse_token_list(*property_line_number, value)?)
            }
            "position" => position = Some(parse_position(*property_line_number, value)?),
            other => {
                return Err(ValidationError::new(format!(
                    "unsupported property `{other}` at line {property_line_number}"
                )))
            }
        }
    }

    let id = id.ok_or_else(|| {
        ValidationError::new(format!("component at line {line_number} must declare `id`"))
    })?;
    let width = width.ok_or_else(|| {
        ValidationError::new(format!("component {id} must declare `width`"))
    })?;
    let height = height.ok_or_else(|| {
        ValidationError::new(format!("component {id} must declare `height`"))
    })?;

    if position.is_some() && (width == Dimension::Fill || height == Dimension::Fill) {
        return Err(ValidationError::new(format!(
            "component {id}: `position` requires fixed `width`/`height` — Fill is flow-only"
        )));
    }

    let kind = match component_kind {
        ComponentKind::CriticalButton => NodeKind::CriticalButton {
            requirement_id: requirement_id.ok_or_else(|| {
                ValidationError::new(format!(
                    "CriticalButton {id} must declare `requirement`"
                ))
            })?,
            label_text_key: label_text_key.ok_or_else(|| {
                ValidationError::new(format!("CriticalButton {id} must declare `label`"))
            })?,
            color_token: color_token.ok_or_else(|| {
                ValidationError::new(format!("CriticalButton {id} must declare `color`"))
            })?,
            on_press: on_press.ok_or_else(|| {
                ValidationError::new(format!("CriticalButton {id} must declare `on_press`"))
            })?,
        },
        ComponentKind::VulkanViewport => NodeKind::VulkanViewport {
            stream_source: stream_source.ok_or_else(|| {
                ValidationError::new(format!(
                    "VulkanViewport {id} must declare `stream_source`"
                ))
            })?,
        },
        ComponentKind::Label => NodeKind::Label {
            text_key: text_key.ok_or_else(|| {
                ValidationError::new(format!("Label {id} must declare `text`"))
            })?,
            color_token: color_token.ok_or_else(|| {
                ValidationError::new(format!("Label {id} must declare `color`"))
            })?,
        },
        ComponentKind::Clock => NodeKind::Clock {
            format: clock_format.ok_or_else(|| {
                ValidationError::new(format!("Clock {id} must declare `format`"))
            })?,
        },
        ComponentKind::NumericDisplay => NodeKind::NumericDisplay {
            requirement_id: requirement_id.ok_or_else(|| {
                ValidationError::new(format!(
                    "NumericDisplay {id} must declare `requirement`"
                ))
            })?,
            template_id: template_id.ok_or_else(|| {
                ValidationError::new(format!("NumericDisplay {id} must declare `template`"))
            })?,
            source: source.ok_or_else(|| {
                ValidationError::new(format!("NumericDisplay {id} must declare `source`"))
            })?,
            color_token: color_token.ok_or_else(|| {
                ValidationError::new(format!("NumericDisplay {id} must declare `color`"))
            })?,
        },
        ComponentKind::StatusIndicator => {
            let state_text_keys = state_text_keys.ok_or_else(|| {
                ValidationError::new(format!("StatusIndicator {id} must declare `states`"))
            })?;
            // `colors` is optional: absent means every state uses the neutral status token.
            let color_tokens = color_tokens.unwrap_or_else(|| {
                vec!["Theme.Colors.Neutral".to_string(); state_text_keys.len()]
            });
            if color_tokens.len() != state_text_keys.len() {
                return Err(ValidationError::new(format!(
                    "StatusIndicator {id} declares {} states but {} colors",
                    state_text_keys.len(),
                    color_tokens.len()
                )));
            }
            NodeKind::StatusIndicator {
                requirement_id: requirement_id.ok_or_else(|| {
                    ValidationError::new(format!(
                        "StatusIndicator {id} must declare `requirement`"
                    ))
                })?,
                source: source.ok_or_else(|| {
                    ValidationError::new(format!(
                        "StatusIndicator {id} must declare `source`"
                    ))
                })?,
                state_text_keys,
                color_tokens,
            }
        }
    };

    Ok(NodeDefinition {
        id,
        width,
        height,
        position,
        kind,
        safety_critical,
    })
}

fn parse_identifier(line_number: usize, field_name: &str, raw: &str) -> MduxResult<String> {
    let value = raw.trim();
    if value.is_empty() {
        return Err(ValidationError::new(format!(
            "{field_name} must not be empty at line {line_number}"
        )));
    }
    if !value
        .chars()
        .all(|character| character.is_ascii_alphanumeric() || matches!(character, '_' | '-'))
    {
        return Err(ValidationError::new(format!(
            "{field_name} contains unsupported characters at line {line_number}"
        )));
    }
    Ok(value.to_string())
}

fn parse_non_empty(line_number: usize, field_name: &str, raw: &str) -> MduxResult<String> {
    let value = raw.trim();
    if value.is_empty() {
        return Err(ValidationError::new(format!(
            "{field_name} must not be empty at line {line_number}"
        )));
    }
    Ok(value.to_string())
}

fn parse_quoted(line_number: usize, field_name: &str, raw: &str) -> MduxResult<String> {
    let value = raw.trim();
    if !(value.starts_with('"') && value.ends_with('"')) {
        return Err(ValidationError::new(format!(
            "{field_name} must be a quoted string at line {line_number}"
        )));
    }
    let inner = &value[1..value.len() - 1];
    if inner.trim().is_empty() {
        return Err(ValidationError::new(format!(
            "{field_name} must not be empty at line {line_number}"
        )));
    }
    Ok(inner.to_string())
}

fn parse_text_key(line_number: usize, raw: &str) -> MduxResult<String> {
    let value = raw.trim();
    let inner = value
        .strip_prefix("t(")
        .and_then(|rest| rest.strip_suffix(')'))
        .ok_or_else(|| {
            ValidationError::new(format!(
                "text references must use t(\"key\") at line {line_number}"
            ))
        })?;
    parse_quoted(line_number, "translation key", inner)
}

fn parse_system_event(line_number: usize, raw: &str) -> MduxResult<SystemEvent> {
    match raw.trim() {
        "SystemEvent.NoOp" => Ok(SystemEvent::NoOp),
        "SystemEvent.TriggerHalt" => Ok(SystemEvent::TriggerHalt),
        other => Err(ValidationError::new(format!(
            "unsupported system event `{other}` at line {line_number}"
        ))),
    }
}

fn parse_clock_format(line_number: usize, raw: &str) -> MduxResult<ClockFormat> {
    match raw.trim() {
        "TimeSeconds" => Ok(ClockFormat::TimeSeconds),
        "DateTimeSeconds" => Ok(ClockFormat::DateTimeSeconds),
        other => Err(ValidationError::new(format!(
            "unsupported clock format `{other}` at line {line_number}"
        ))),
    }
}

/// Parses `[a, b, c]` into raw element strings. Splitting on `,` is safe for both list kinds:
/// translation keys and color tokens never contain commas.
fn parse_bracket_list(line_number: usize, field_name: &str, raw: &str) -> MduxResult<Vec<String>> {
    let inner = raw
        .trim()
        .strip_prefix('[')
        .and_then(|rest| rest.strip_suffix(']'))
        .ok_or_else(|| {
            ValidationError::new(format!(
                "{field_name} must be a [..] list at line {line_number}"
            ))
        })?;
    let entries = inner
        .split(',')
        .map(str::trim)
        .filter(|entry| !entry.is_empty())
        .map(str::to_string)
        .collect::<Vec<_>>();
    if entries.is_empty() {
        return Err(ValidationError::new(format!(
            "{field_name} must declare at least one entry at line {line_number}"
        )));
    }
    Ok(entries)
}

fn parse_text_key_list(line_number: usize, raw: &str) -> MduxResult<Vec<String>> {
    parse_bracket_list(line_number, "states", raw)?
        .iter()
        .map(|entry| parse_text_key(line_number, entry))
        .collect()
}

fn parse_token_list(line_number: usize, raw: &str) -> MduxResult<Vec<String>> {
    parse_bracket_list(line_number, "colors", raw)?
        .iter()
        .map(|entry| parse_non_empty(line_number, "color token", entry))
        .collect()
}

fn parse_dimension(line_number: usize, field_name: &str, raw: &str) -> MduxResult<Dimension> {
    match raw.trim() {
        "Fill" => Ok(Dimension::Fill),
        other => parse_px(line_number, field_name, other).map(|value| Dimension::Px(u32::from(value))),
    }
}

fn parse_px(line_number: usize, field_name: &str, raw: &str) -> MduxResult<u16> {
    let px_value = parse_px_allowing_zero(line_number, field_name, raw)?;
    if px_value == 0 {
        return Err(ValidationError::new(format!(
            "{field_name} must be greater than zero at line {line_number}"
        )));
    }
    Ok(px_value)
}

/// Like [`parse_px`] but accepts `0px` — legal for layout spacing/padding and `position`
/// coordinates (ADR-014), never for component sizes.
fn parse_px_allowing_zero(line_number: usize, field_name: &str, raw: &str) -> MduxResult<u16> {
    raw.trim()
        .strip_suffix("px")
        .ok_or_else(|| {
            ValidationError::new(format!(
                "{field_name} must use a px unit at line {line_number}"
            ))
        })?
        .parse::<u16>()
        .map_err(|_| {
            ValidationError::new(format!(
                "{field_name} must be a non-negative integer px value at line {line_number}"
            ))
        })
}

/// Parses `<X>px, <Y>px` — the absolute screen coordinates of a positioned component.
fn parse_position(line_number: usize, raw: &str) -> MduxResult<(u32, u32)> {
    let (x_raw, y_raw) = raw.split_once(',').ok_or_else(|| {
        ValidationError::new(format!(
            "position must be `<X>px, <Y>px` at line {line_number}"
        ))
    })?;
    let x = parse_px_allowing_zero(line_number, "position x", x_raw)?;
    let y = parse_px_allowing_zero(line_number, "position y", y_raw)?;
    Ok((u32::from(x), u32::from(y)))
}

/// Parses `<W>px, <H>px` — the screen's declared surface pin.
fn parse_surface(line_number: usize, raw: &str) -> MduxResult<(u32, u32)> {
    let (w_raw, h_raw) = raw.split_once(',').ok_or_else(|| {
        ValidationError::new(format!(
            "surface must be `<W>px, <H>px` at line {line_number}"
        ))
    })?;
    let width = parse_px(line_number, "surface width", w_raw)?;
    let height = parse_px(line_number, "surface height", h_raw)?;
    Ok((u32::from(width), u32::from(height)))
}

fn compile_screen(
    screen: ScreenDefinition,
    options: CompileOptions,
    text_packages: TextPackages<'_>,
) -> MduxResult<CompiledScreenSpec> {
    if options.surface_width == 0 || options.surface_height == 0 {
        return Err(ValidationError::new(
            "compile options surface dimensions must be greater than zero",
        ));
    }

    if let Some((declared_width, declared_height)) = screen.declared_surface {
        if (declared_width, declared_height) != (options.surface_width, options.surface_height) {
            return Err(ValidationError::new(format!(
                "screen {} declares surface {declared_width}x{declared_height} but the build configured {}x{} — align build.rs .surface(...) with the declaration",
                screen.id, options.surface_width, options.surface_height
            )));
        }
    }

    let content_width = options
        .surface_width
        .checked_sub(u32::from(screen.layout.padding) * 2)
        .ok_or_else(|| ValidationError::new("layout padding exceeds surface width"))?;
    let content_height = options
        .surface_height
        .checked_sub(u32::from(screen.layout.padding) * 2)
        .ok_or_else(|| ValidationError::new("layout padding exceeds surface height"))?;

    let has_rows = screen
        .items
        .iter()
        .any(|item| matches!(item, ScreenItem::Row(_)));
    if has_rows && screen.layout.kind != LayoutKind::Vertical {
        return Err(ValidationError::new(
            "Row containers require a Vertical screen layout",
        ));
    }

    // Axis resolution over top-level FLOW items only: positioned nodes are out of flow
    // (ADR-014) — Fill siblings distribute space as if they did not exist. A Row contributes
    // its own height and spans the content width; a leaf component its declared dimensions.
    let is_flow = |item: &ScreenItem| match item {
        ScreenItem::Component(node) => node.position.is_none(),
        ScreenItem::Row(_) => true,
    };
    let resolved_widths = resolve_axis_sizes(
        screen.items.iter().filter(|item| is_flow(item)).map(|item| match item {
            ScreenItem::Component(node) => node.width,
            ScreenItem::Row(_) => Dimension::Fill,
        }),
        if screen.layout.kind == LayoutKind::Horizontal {
            content_width
        } else {
            0
        },
        usize::from(screen.layout.spacing),
    )?;
    let resolved_heights = resolve_axis_sizes(
        screen.items.iter().filter(|item| is_flow(item)).map(|item| match item {
            ScreenItem::Component(node) => node.height,
            ScreenItem::Row(row) => row.height,
        }),
        if screen.layout.kind == LayoutKind::Vertical {
            content_height
        } else {
            0
        },
        usize::from(screen.layout.spacing),
    )?;

    let mut cursor_x = i32::from(screen.layout.padding);
    let mut cursor_y = i32::from(screen.layout.padding);
    let mut nodes = Vec::new();
    let mut golden_references = Vec::new();
    let screen_id = screen.id.clone();
    let layout = screen.layout.clone();
    let spacing = i32::from(layout.spacing);
    let padding = i32::from(layout.padding);

    let mut context = CompileContext {
        options,
        text_packages,
        padding,
        nodes: &mut nodes,
        golden_references: &mut golden_references,
        positioned_indices: Vec::new(),
    };

    let mut flow_index = 0usize;
    for item in screen.items.into_iter() {
        match item {
            ScreenItem::Component(node) => {
                if let Some((x, y)) = node.position {
                    // Positioned top-level node: out of flow, contained by the padded content
                    // box (compile_leaf's ordinary containment check enforces exactly that).
                    let bounds = RectSpec {
                        x: x as i32,
                        y: y as i32,
                        width: fixed_dimension(&node, node.width)?,
                        height: fixed_dimension(&node, node.height)?,
                    };
                    context.compile_leaf(node, bounds)?;
                    continue;
                }

                let width = match node.width {
                    Dimension::Fill if layout.kind == LayoutKind::Vertical => content_width,
                    Dimension::Fill => resolved_widths[flow_index],
                    Dimension::Px(value) => u32::from(value),
                };
                let height = match node.height {
                    Dimension::Fill if layout.kind == LayoutKind::Horizontal => content_height,
                    Dimension::Fill => resolved_heights[flow_index],
                    Dimension::Px(value) => u32::from(value),
                };
                let bounds = RectSpec {
                    x: cursor_x,
                    y: cursor_y,
                    width,
                    height,
                };
                context.compile_leaf(node, bounds)?;

                match layout.kind {
                    LayoutKind::Vertical => cursor_y += bounds.height as i32 + spacing,
                    LayoutKind::Horizontal => cursor_x += bounds.width as i32 + spacing,
                }
                flow_index += 1;
            }
            ScreenItem::Row(row) => {
                let row_height = match row.height {
                    Dimension::Fill => resolved_heights[flow_index],
                    Dimension::Px(value) => u32::from(value),
                };
                let row_bounds = RectSpec {
                    x: padding,
                    y: cursor_y,
                    width: content_width,
                    height: row_height,
                };

                // Row background: a synthetic Panel underlay spanning the whole row, pushed
                // before the row's children (ADR-014). Panels are overlap-exempt.
                if let Some(color_token) = &row.background {
                    context.push_panel(&row.id, color_token.clone(), row_bounds)?;
                }

                // Horizontal resolution of the row's FLOW children across the content width;
                // positioned children are out of flow.
                let child_widths = resolve_axis_sizes(
                    row.children
                        .iter()
                        .filter(|child| child.position.is_none())
                        .map(|child| child.width),
                    content_width,
                    usize::from(row.spacing),
                )?;
                let row_spacing = i32::from(row.spacing);
                let mut child_x = i32::from(screen.layout.padding);
                let mut child_flow_index = 0usize;

                for child in row.children.into_iter() {
                    if let Some((x, y)) = child.position {
                        // Positioned row child: must lie entirely inside its declaring Row.
                        let bounds = RectSpec {
                            x: x as i32,
                            y: y as i32,
                            width: fixed_dimension(&child, child.width)?,
                            height: fixed_dimension(&child, child.height)?,
                        };
                        if !rect_contains(row_bounds, bounds) {
                            return Err(ValidationError::new(format!(
                                "component {} (position {},{}, size {}x{}) escapes its Row {} (bounds {},{} {}x{})",
                                child.id, bounds.x, bounds.y, bounds.width, bounds.height,
                                row.id, row_bounds.x, row_bounds.y, row_bounds.width,
                                row_bounds.height
                            )));
                        }
                        context.compile_leaf(child, bounds)?;
                        continue;
                    }

                    let child_width = child_widths[child_flow_index];
                    let child_height = match child.height {
                        Dimension::Fill => row_height,
                        Dimension::Px(value) => u32::from(value),
                    };
                    if child_height > row_height {
                        return Err(ValidationError::new(format!(
                            "component {} is taller than its Row {}",
                            child.id, row.id
                        )));
                    }
                    let bounds = RectSpec {
                        x: child_x,
                        y: cursor_y,
                        width: child_width,
                        height: child_height,
                    };
                    context.compile_leaf(child, bounds)?;
                    child_x += child_width as i32 + row_spacing;
                    child_flow_index += 1;
                }

                cursor_y += row_height as i32 + spacing;
                flow_index += 1;
            }
        }
    }

    let positioned_indices = context.positioned_indices.clone();
    validate_unique_node_ids(&nodes)?;
    validate_no_overlap(&nodes, &positioned_indices)?;

    Ok(CompiledScreenSpec {
        id: screen_id,
        layout,
        surface: (options.surface_width, options.surface_height),
        nodes,
        golden_references,
    })
}

/// A positioned component's dimensions are guaranteed `Px` by the parser; this converts them
/// defensively.
fn fixed_dimension(node: &NodeDefinition, dimension: Dimension) -> MduxResult<u32> {
    match dimension {
        Dimension::Px(value) => Ok(u32::from(value)),
        Dimension::Fill => Err(ValidationError::new(format!(
            "component {}: `position` requires fixed `width`/`height` — Fill is flow-only",
            node.id
        ))),
    }
}

/// `inner` lies entirely within `outer` (shared edges allowed).
fn rect_contains(outer: RectSpec, inner: RectSpec) -> bool {
    inner.x >= outer.x
        && inner.y >= outer.y
        && inner.x + inner.width as i32 <= outer.x + outer.width as i32
        && inner.y + inner.height as i32 <= outer.y + outer.height as i32
}

/// Strict AABB intersection: shared edges are legal adjacency, not overlap.
fn rects_strictly_overlap(a: RectSpec, b: RectSpec) -> bool {
    a.x < b.x + b.width as i32
        && b.x < a.x + a.width as i32
        && a.y < b.y + b.height as i32
        && b.y < a.y + a.height as i32
}

/// Every compiled node id must be unique — including the synthesized `{row_id}-background`
/// Panel ids, so a user id colliding with one is a compile error rather than silent.
fn validate_unique_node_ids(nodes: &[CompiledNodeSpec]) -> MduxResult<()> {
    let mut seen = std::collections::BTreeSet::new();
    for node in nodes {
        if !seen.insert(node.id.as_str()) {
            return Err(ValidationError::new(format!(
                "duplicate node id {} in compiled screen",
                node.id
            )));
        }
    }
    Ok(())
}

/// ADR-014 verification rule 2: a positioned node must not overlap ANY other node, flow or
/// positioned. Panels are exempt on both sides (backgrounds are underlays by definition).
fn validate_no_overlap(
    nodes: &[CompiledNodeSpec],
    positioned_indices: &[usize],
) -> MduxResult<()> {
    for &positioned in positioned_indices {
        let a = &nodes[positioned];
        if matches!(a.kind, NodeKind::Panel { .. }) {
            continue;
        }
        for (index, b) in nodes.iter().enumerate() {
            if index == positioned || matches!(b.kind, NodeKind::Panel { .. }) {
                continue;
            }
            if rects_strictly_overlap(a.bounds, b.bounds) {
                return Err(ValidationError::new(format!(
                    "component {} (position {},{}, size {}x{}) overlaps component {} (bounds {},{} {}x{}); positioned components must not overlap any other component",
                    a.id, a.bounds.x, a.bounds.y, a.bounds.width, a.bounds.height,
                    b.id, b.bounds.x, b.bounds.y, b.bounds.width, b.bounds.height
                )));
            }
        }
    }
    Ok(())
}

/// Shared leaf-node compilation: surface containment, text budgets, theme-token validation,
/// golden-reference emission, positioned-node tracking for the overlap pass.
struct CompileContext<'a, 'p> {
    options: CompileOptions,
    text_packages: TextPackages<'p>,
    padding: i32,
    nodes: &'a mut Vec<CompiledNodeSpec>,
    golden_references: &'a mut Vec<GoldenReferenceSpec>,
    positioned_indices: Vec<usize>,
}

impl CompileContext<'_, '_> {
    fn compile_leaf(&mut self, node: NodeDefinition, bounds: RectSpec) -> MduxResult<()> {
        if bounds.x < self.padding || bounds.y < self.padding {
            return Err(ValidationError::new(format!(
                "component {} resolved outside the padded surface",
                node.id
            )));
        }
        if bounds.x + bounds.width as i32 > self.options.surface_width as i32 - self.padding
            || bounds.y + bounds.height as i32
                > self.options.surface_height as i32 - self.padding
        {
            return Err(ValidationError::new(format!(
                "component {} exceeds the available surface",
                node.id
            )));
        }

        validate_node_text_budget(&node, bounds, self.text_packages)?;
        validate_color_tokens(&node.id, &node.kind)?;

        // ADR-014 rule 4: a positioned node's declared placement is golden evidence — it gets
        // an automatic Bounds reference even without @safety_critical. When both apply, ONE
        // merged entry is emitted (deduplicated cv_check union), never two per node id.
        let is_positioned = node.position.is_some();
        if is_positioned || node.safety_critical.is_some() {
            let mut cv_checks = node
                .safety_critical
                .as_ref()
                .map(|safety| safety.cv_checks.clone())
                .unwrap_or_default();
            if is_positioned && !cv_checks.contains(&CvCheckKind::Bounds) {
                cv_checks.insert(0, CvCheckKind::Bounds);
            }

            let (text_key, color_token) = match &node.kind {
                NodeKind::CriticalButton {
                    label_text_key,
                    color_token,
                    ..
                } => (Some(label_text_key.clone()), Some(color_token.clone())),
                NodeKind::Label {
                    text_key,
                    color_token,
                } => (Some(text_key.clone()), Some(color_token.clone())),
                // Dynamic content: the golden reference pins bounds (and color for the numeric
                // display); the rendered text varies at runtime by design.
                NodeKind::NumericDisplay { color_token, .. } => {
                    (None, Some(color_token.clone()))
                }
                NodeKind::StatusIndicator { .. }
                | NodeKind::Clock { .. }
                | NodeKind::VulkanViewport { .. }
                | NodeKind::Panel { .. } => (None, None),
            };
            golden_references_push(
                self.golden_references,
                &node,
                bounds,
                text_key,
                color_token,
                cv_checks,
            );
        }

        if is_positioned {
            self.positioned_indices.push(self.nodes.len());
        }
        self.nodes.push(CompiledNodeSpec {
            id: node.id,
            bounds,
            kind: node.kind,
        });

        Ok(())
    }

    /// Pushes the synthetic `{row_id}-background` Panel underlay for a Row's `background:`.
    fn push_panel(
        &mut self,
        row_id: &str,
        color_token: String,
        bounds: RectSpec,
    ) -> MduxResult<()> {
        let id = format!("{row_id}-background");
        validate_color_tokens(&id, &NodeKind::Panel { color_token: color_token.clone() })?;
        self.nodes.push(CompiledNodeSpec {
            id,
            bounds,
            kind: NodeKind::Panel { color_token },
        });
        Ok(())
    }
}

/// ADR-014: every color-bearing property must name a token from the governed theme table.
fn validate_color_tokens(node_id: &str, kind: &NodeKind) -> MduxResult<()> {
    let check = |token: &str| -> MduxResult<()> {
        if resolve_color_token(token).is_none() {
            let approved = THEME_COLORS
                .iter()
                .map(|(name, _)| *name)
                .collect::<Vec<_>>()
                .join(", ");
            return Err(ValidationError::new(format!(
                "component {node_id} references unknown theme color token `{token}`; approved tokens are: {approved}"
            )));
        }
        Ok(())
    };

    match kind {
        NodeKind::CriticalButton { color_token, .. }
        | NodeKind::Label { color_token, .. }
        | NodeKind::NumericDisplay { color_token, .. }
        | NodeKind::Panel { color_token } => check(color_token),
        NodeKind::StatusIndicator { color_tokens, .. } => {
            for token in color_tokens {
                check(token)?;
            }
            Ok(())
        }
        NodeKind::Clock { .. } | NodeKind::VulkanViewport { .. } => Ok(()),
    }
}

fn golden_references_push(
    golden_references: &mut Vec<GoldenReferenceSpec>,
    node: &NodeDefinition,
    bounds: RectSpec,
    text_key: Option<String>,
    color_token: Option<String>,
    cv_checks: Vec<CvCheckKind>,
) {
    golden_references.push(GoldenReferenceSpec {
        node_id: node.id.clone(),
        bounds,
        text_key,
        color_token,
        cv_checks,
    });
}

fn validate_node_text_budget(
    node: &NodeDefinition,
    bounds: RectSpec,
    text_packages: TextPackages<'_>,
) -> MduxResult<()> {
    match &node.kind {
        NodeKind::CriticalButton { label_text_key, .. } => {
            validate_static_text_budget(node, label_text_key, bounds, text_packages.standard)
        }
        NodeKind::Label { text_key, .. } => {
            validate_static_text_budget(node, text_key, bounds, text_packages.standard)
        }
        NodeKind::StatusIndicator {
            state_text_keys, ..
        } => {
            // Every state label, in every approved locale, must fit — the widest translation of
            // the widest state defines the budget.
            for state_text_key in state_text_keys {
                validate_static_text_budget(node, state_text_key, bounds, text_packages.standard)?;
            }
            Ok(())
        }
        NodeKind::Clock { format } => {
            validate_clock_budget(node, *format, bounds, text_packages.standard)
        }
        // Panels carry no text.
        NodeKind::Panel { .. } => Ok(()),
        NodeKind::NumericDisplay { template_id, .. } => {
            let display = text_packages.display.ok_or_else(|| {
                ValidationError::new(format!(
                    "NumericDisplay {} requires a display text package (none was provided to the compiler)",
                    node.id
                ))
            })?;
            validate_numeric_display_budget(node, template_id, bounds, display)
        }
        NodeKind::VulkanViewport { .. } => Ok(()),
    }
}

fn validate_static_text_budget(
    node: &NodeDefinition,
    text_key: &str,
    bounds: RectSpec,
    text_package: &TextPackage,
) -> MduxResult<()> {
    let locales = text_package
        .approved_strings
        .iter()
        .filter(|approved_string| approved_string.id == *text_key)
        .map(|approved_string| approved_string.locale.as_str())
        .collect::<Vec<_>>();

    if locales.is_empty() {
        return Err(ValidationError::new(format!(
            "text key {text_key} for component {} does not exist in the approved text package",
            node.id
        )));
    }

    for locale in locales {
        let run = text_package
            .find_run_for_string(text_key, locale)
            .ok_or_else(|| {
                ValidationError::new(format!(
                    "text key {text_key} for component {} is missing a compiled run for locale {locale}",
                    node.id
                ))
            })?;
        let run_bounds = measure_text_run_bounds(text_package, run)?;
        if run_bounds.width() > bounds.width || run_bounds.height() > bounds.height {
            return Err(ValidationError::new(format!(
                "text key {text_key} for component {} exceeds bounds in locale {locale}: required width={} height={}, available width={} height={}",
                node.id,
                run_bounds.width(),
                run_bounds.height(),
                bounds.width,
                bounds.height
            )));
        }
    }

    Ok(())
}

/// The clock renders a fixed glyph sequence (`HH:MM:SS`, optionally preceded by
/// `YYYY-MM-DD `), so its budget is exactly computable from the glyph set's advances and
/// glyph extents.
fn validate_clock_budget(
    node: &NodeDefinition,
    format: ClockFormat,
    bounds: RectSpec,
    text_package: &TextPackage,
) -> MduxResult<()> {
    let glyph_set = text_package
        .find_numeric_glyph_set(CLOCK_GLYPH_SET_ID)
        .ok_or_else(|| {
            ValidationError::new(format!(
                "Clock {} requires glyph set {CLOCK_GLYPH_SET_ID} in the approved text package",
                node.id
            ))
        })?;

    let characters: &[char] = match format {
        ClockFormat::TimeSeconds => &['0', '0', ':', '0', '0', ':', '0', '0'],
        ClockFormat::DateTimeSeconds => &[
            '0', '0', '0', '0', '-', '0', '0', '-', '0', '0', ' ', '0', '0', ':', '0', '0',
            ':', '0', '0',
        ],
    };

    let (required_width, required_height) =
        measure_glyph_run(node, glyph_set, characters, text_package)?;

    if required_width > bounds.width || required_height > bounds.height {
        return Err(ValidationError::new(format!(
            "Clock {} does not fit its bounds: required width={required_width} height={required_height}, available width={} height={}",
            node.id, bounds.width, bounds.height
        )));
    }

    Ok(())
}

/// A numeric display renders up to `max_chars` digits (plus optional affix runs); the widest
/// digit of the template's glyph set defines the worst case.
fn validate_numeric_display_budget(
    node: &NodeDefinition,
    template_id: &str,
    bounds: RectSpec,
    display_package: &TextPackage,
) -> MduxResult<()> {
    let template = display_package.find_template(template_id).ok_or_else(|| {
        ValidationError::new(format!(
            "NumericDisplay {} references unknown template {template_id} in the display package",
            node.id
        ))
    })?;
    let glyph_set = display_package
        .find_numeric_glyph_set(&template.glyph_set_id)
        .ok_or_else(|| {
            ValidationError::new(format!(
                "NumericDisplay {} template references unknown glyph set {}",
                node.id, template.glyph_set_id
            ))
        })?;

    // Worst case: max_chars occurrences of the widest digit (advance-wise).
    let widest = glyph_set
        .entries
        .iter()
        .max_by_key(|entry| entry.advance_x)
        .ok_or_else(|| {
            ValidationError::new(format!(
                "NumericDisplay {} template glyph set is empty",
                node.id
            ))
        })?;
    let characters = vec![widest.character; usize::from(template.max_chars)];
    let (mut required_width, mut required_height) =
        measure_glyph_run(node, glyph_set, &characters, display_package)?;

    for affix_run_id in [&template.prefix_run_id, &template.suffix_run_id]
        .into_iter()
        .flatten()
    {
        let run = display_package.find_run(affix_run_id).ok_or_else(|| {
            ValidationError::new(format!(
                "NumericDisplay {} template references unknown affix run {affix_run_id}",
                node.id
            ))
        })?;
        let run_bounds = display_package.measure_run_bounds(run)?;
        required_width += run_bounds.width();
        required_height = required_height.max(run_bounds.height());
    }

    if required_width > bounds.width || required_height > bounds.height {
        return Err(ValidationError::new(format!(
            "NumericDisplay {} does not fit its bounds: required width={required_width} height={required_height}, available width={} height={}",
            node.id, bounds.width, bounds.height
        )));
    }

    Ok(())
}

/// Measures the pixel extents of a glyph-set character sequence: width is the sum of advances,
/// height the tallest glyph. Missing characters are compile errors.
fn measure_glyph_run(
    node: &NodeDefinition,
    glyph_set: &NumericGlyphSet,
    characters: &[char],
    text_package: &TextPackage,
) -> MduxResult<(u32, u32)> {
    let mut width: u32 = 0;
    let mut height: u32 = 0;

    for character in characters {
        let entry = glyph_set
            .entries
            .iter()
            .find(|entry| entry.character == *character)
            .ok_or_else(|| {
                ValidationError::new(format!(
                    "component {} requires character '{character}' missing from glyph set {}",
                    node.id, glyph_set.id
                ))
            })?;
        width = width.saturating_add(entry.advance_x.max(0) as u32);
        let atlas_glyph = text_package
            .find_glyph(entry.atlas_index, entry.glyph_id)
            .ok_or_else(|| {
                ValidationError::new(format!(
                    "component {} references glyph set {} entry '{character}', but atlas index {} glyph {} does not exist in the package — under-measuring height would let an out-of-budget component compile",
                    node.id, glyph_set.id, entry.atlas_index, entry.glyph_id
                ))
            })?;
        height = height.max(u32::from(atlas_glyph.height));
    }

    Ok((width, height))
}

fn measure_text_run_bounds(
    text_package: &TextPackage,
    run: &CompiledTextRun,
) -> MduxResult<mdux_text_schema::TextRunBounds> {
    text_package.measure_run_bounds(run)
}

fn resolve_axis_sizes(
    dimensions: impl Iterator<Item = Dimension>,
    total_fill_space: u32,
    spacing: usize,
) -> MduxResult<Vec<u32>> {
    let dimensions = dimensions.collect::<Vec<_>>();
    if total_fill_space == 0 {
        return Ok(dimensions
            .into_iter()
            .map(|dimension| match dimension {
                Dimension::Px(value) => u32::from(value),
                Dimension::Fill => 0,
            })
            .collect());
    }

    let fixed_total = dimensions
        .iter()
        .map(|dimension| match dimension {
            Dimension::Px(value) => u32::from(*value),
            Dimension::Fill => 0,
        })
        .sum::<u32>();
    let fill_count = dimensions
        .iter()
        .filter(|dimension| matches!(dimension, Dimension::Fill))
        .count() as u32;
    let total_spacing = spacing as u32 * dimensions.len().saturating_sub(1) as u32;
    let available = total_fill_space
        .checked_sub(fixed_total + total_spacing)
        .ok_or_else(|| ValidationError::new("layout exceeds available surface"))?;

    let fill_size = if fill_count == 0 { 0 } else { available / fill_count };
    if fill_count > 0 && fill_size == 0 {
        return Err(ValidationError::new(
            "Fill layout items do not have enough remaining space",
        ));
    }

    Ok(dimensions
        .into_iter()
        .map(|dimension| match dimension {
            Dimension::Px(value) => u32::from(value),
            Dimension::Fill => fill_size,
        })
        .collect())
}

fn emit_rust_module(compiled: &CompiledScreenSpec, crate_path: &str) -> String {
    let mut output = String::new();
    let primary_text_node_id = compiled
        .nodes
        .iter()
        .find_map(|node| matches!(&node.kind, NodeKind::CriticalButton { .. }).then_some(node.id.as_str()))
        .unwrap_or("");
    let _ = writeln!(
        output,
        "pub const GENERATED_PRIMARY_TEXT_NODE_ID: &str = {primary_text_node_id:?};"
    );
    let _ = writeln!(
        output,
        "pub const GENERATED_MEDUI_SURFACE: (u32, u32) = ({}, {});",
        compiled.surface.0, compiled.surface.1
    );
    let _ = writeln!(
        output,
        "pub const GENERATED_MEDUI_PACKAGE: {crate_path}::CompiledScreenPackage = {crate_path}::CompiledScreenPackage {{"
    );
    let _ = writeln!(output, "    screen_id: {:?},", compiled.id);
    let _ = writeln!(
        output,
        "    layout: {crate_path}::LayoutSpec {{ kind: {}, spacing: {}, padding: {} }},",
        emit_layout_kind(compiled.layout.kind, crate_path),
        compiled.layout.spacing,
        compiled.layout.padding
    );
    let _ = writeln!(output, "    nodes: &[");
    for node in &compiled.nodes {
        let _ = writeln!(output, "        {crate_path}::CompiledNode {{");
        let _ = writeln!(output, "            id: {:?},", node.id);
        let _ = writeln!(
            output,
            "            bounds: {crate_path}::Rect {{ x: {}, y: {}, width: {}, height: {} }},",
            node.bounds.x, node.bounds.y, node.bounds.width, node.bounds.height
        );
        let _ = writeln!(
            output,
            "            kind: {},",
            emit_node_kind(&node.kind, crate_path)
        );
        let _ = writeln!(output, "        }},");
    }
    let _ = writeln!(output, "    ],");
    let _ = writeln!(output, "    golden_references: &[");
    for golden_reference in &compiled.golden_references {
        let _ = writeln!(output, "        {crate_path}::GoldenReferenceEntry {{");
        let _ = writeln!(output, "            node_id: {:?},", golden_reference.node_id);
        let _ = writeln!(
            output,
            "            bounds: {crate_path}::Rect {{ x: {}, y: {}, width: {}, height: {} }},",
            golden_reference.bounds.x,
            golden_reference.bounds.y,
            golden_reference.bounds.width,
            golden_reference.bounds.height
        );
        let _ = writeln!(
            output,
            "            text_key: {},",
            emit_optional_string(golden_reference.text_key.as_deref())
        );
        let _ = writeln!(
            output,
            "            color_token: {},",
            emit_optional_string(golden_reference.color_token.as_deref())
        );
        let _ = writeln!(
            output,
            "            cv_checks: {},",
            emit_cv_checks(&golden_reference.cv_checks, crate_path)
        );
        let _ = writeln!(output, "        }},");
    }
    let _ = writeln!(output, "    ],");
    let _ = writeln!(output, "}};");
    let _ = writeln!(
        output,
        "pub fn screen() -> &'static {crate_path}::CompiledScreenPackage {{ &GENERATED_MEDUI_PACKAGE }}"
    );
    let _ = writeln!(
        output,
        "pub fn primary_text_node_id() -> &'static str {{ GENERATED_PRIMARY_TEXT_NODE_ID }}"
    );
    output
}

fn emit_layout_kind(kind: LayoutKind, crate_path: &str) -> String {
    match kind {
        LayoutKind::Vertical => format!("{crate_path}::LayoutKind::Vertical"),
        LayoutKind::Horizontal => format!("{crate_path}::LayoutKind::Horizontal"),
    }
}

fn emit_node_kind(kind: &NodeKind, crate_path: &str) -> String {
    match kind {
        NodeKind::CriticalButton {
            requirement_id,
            label_text_key,
            color_token,
            on_press,
        } => format!(
            "{crate_path}::CompiledNodeKind::CriticalButton({crate_path}::CriticalButtonSpec {{ requirement_id: {requirement_id:?}, text_key: {label_text_key:?}, color_token: {color_token:?}, on_press: {} }})",
            emit_system_event(*on_press, crate_path)
        ),
        NodeKind::VulkanViewport { stream_source } => format!(
            "{crate_path}::CompiledNodeKind::VulkanViewport({crate_path}::ViewportReservation {{ stream_source: {stream_source:?} }})"
        ),
        NodeKind::Label {
            text_key,
            color_token,
        } => format!(
            "{crate_path}::CompiledNodeKind::Label({crate_path}::LabelSpec {{ text_key: {text_key:?}, color_token: {color_token:?} }})"
        ),
        NodeKind::Clock { format } => format!(
            "{crate_path}::CompiledNodeKind::Clock({crate_path}::ClockSpec {{ format: {} }})",
            emit_clock_format(*format, crate_path)
        ),
        NodeKind::NumericDisplay {
            requirement_id,
            template_id,
            source,
            color_token,
        } => format!(
            "{crate_path}::CompiledNodeKind::NumericDisplay({crate_path}::NumericDisplaySpec {{ requirement_id: {requirement_id:?}, template_id: {template_id:?}, source: {source:?}, color_token: {color_token:?} }})"
        ),
        NodeKind::StatusIndicator {
            requirement_id,
            source,
            state_text_keys,
            color_tokens,
        } => format!(
            "{crate_path}::CompiledNodeKind::StatusIndicator({crate_path}::StatusIndicatorSpec {{ requirement_id: {requirement_id:?}, source: {source:?}, state_text_keys: {}, color_tokens: {} }})",
            emit_str_slice(state_text_keys),
            emit_str_slice(color_tokens)
        ),
        NodeKind::Panel { color_token } => format!(
            "{crate_path}::CompiledNodeKind::Panel({crate_path}::PanelSpec {{ color_token: {color_token:?} }})"
        ),
    }
}

fn emit_clock_format(format: ClockFormat, crate_path: &str) -> String {
    match format {
        ClockFormat::TimeSeconds => format!("{crate_path}::ClockFormat::TimeSeconds"),
        ClockFormat::DateTimeSeconds => format!("{crate_path}::ClockFormat::DateTimeSeconds"),
    }
}

fn emit_str_slice(values: &[String]) -> String {
    format!(
        "&[{}]",
        values
            .iter()
            .map(|value| format!("{value:?}"))
            .collect::<Vec<_>>()
            .join(", ")
    )
}

fn emit_system_event(event: SystemEvent, crate_path: &str) -> String {
    match event {
        SystemEvent::NoOp => format!("{crate_path}::SystemEvent::NoOp"),
        SystemEvent::TriggerHalt => format!("{crate_path}::SystemEvent::TriggerHalt"),
    }
}

fn emit_optional_string(value: Option<&str>) -> String {
    value
        .map(|entry| format!("Some({entry:?})"))
        .unwrap_or_else(|| "None".to_string())
}

fn emit_cv_checks(checks: &[CvCheckKind], crate_path: &str) -> String {
    if checks.is_empty() {
        "&[]".to_string()
    } else {
        format!(
            "&[{}]",
            checks
                .iter()
                .map(|check| match check {
                    CvCheckKind::Bounds => format!("{crate_path}::CvCheckKind::Bounds"),
                    CvCheckKind::ColorHash => format!("{crate_path}::CvCheckKind::ColorHash"),
                })
                .collect::<Vec<_>>()
                .join(", ")
        )
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use mdux_text_schema::{
        ApprovedString, AtlasGlyph, CompiledGlyph, DeterminismEvidence, FontAsset, TextDirection,
        TextPackage, TextureAtlas,
    };

    const SAMPLE_MEDUI: &str = r#"
Screen HelloWorld {
    layout: Vertical { spacing: 16px; padding: 24px; }

    @safety_critical(cv_check: [Bounds, ColorHash])
    CriticalButton {
        id: hello-world-label;
        requirement: "REQ-HELLO-001";
        width: Fill;
        height: 80px;
        label: t("STR-HELLO-WORLD");
        color: Theme.Colors.PrimaryAction;
        on_press: SystemEvent.NoOp;
    }

    VulkanViewport {
        id: hello-world-viewport;
        width: Fill;
        height: 280px;
        stream_source: "HELLO_WORLD_SIM";
    }
}
"#;

    #[test]
    fn emits_generated_package_for_minimal_vertical_screen() {
        let generated = compile_medui_source_to_rust(
            SAMPLE_MEDUI,
            CompileOptions::new(800, 480),
            TextPackages::standard_only(&sample_text_package()),
        )
        .expect("sample medui should compile");

        assert!(generated.contains("pub const GENERATED_MEDUI_PACKAGE"));
        assert!(generated.contains("screen_id: \"HelloWorld\""));
        assert!(generated.contains("id: \"hello-world-label\""));
        assert!(generated.contains("stream_source: \"HELLO_WORLD_SIM\""));
        assert!(generated.contains(
            "cv_checks: &[::mdux::CvCheckKind::Bounds, ::mdux::CvCheckKind::ColorHash]"
        ));
        assert!(generated.contains("pub fn screen() -> &'static ::mdux::CompiledScreenPackage"));
        assert!(generated.contains("pub fn primary_text_node_id() -> &'static str"));
    }

    #[test]
    fn rejects_missing_requirement_binding() {
        let source = SAMPLE_MEDUI.replace("        requirement: \"REQ-HELLO-001\";\n", "");
        let error = compile_medui_source_to_rust(
            &source,
            CompileOptions::new(800, 480),
            TextPackages::standard_only(&sample_text_package()),
        )
        .expect_err("critical buttons must bind requirements");

        assert_eq!(
            error.to_string(),
            "CriticalButton hello-world-label must declare `requirement`"
        );
    }

    #[test]
    fn rejects_layouts_that_exceed_the_available_surface() {
        let source = SAMPLE_MEDUI.replace("        height: 280px;\n", "        height: 420px;\n");
        let error = compile_medui_source_to_rust(
            &source,
            CompileOptions::new(800, 480),
            TextPackages::standard_only(&sample_text_package()),
        )
        .expect_err("oversized layouts should be rejected");

        assert_eq!(
            error.to_string(),
            "layout exceeds available surface"
        );
    }

    #[test]
    fn rejects_the_widest_translation_that_overflows_button_bounds() {
        let source = SAMPLE_MEDUI.replace("        width: Fill;\n", "        width: 80px;\n");
        let error = compile_medui_source_to_rust(
            &source,
            CompileOptions::new(800, 480),
            TextPackages::standard_only(&sample_text_package()),
        )
        .expect_err("widest translation should be rejected at compile time");

        assert_eq!(
            error.to_string(),
            "text key STR-HELLO-WORLD for component hello-world-label exceeds bounds in locale fr-FR: required width=96 height=16, available width=80 height=80"
        );
    }

    const MONITOR_MEDUI: &str = r#"
Screen NeuroSense500 {
    layout: Vertical { spacing: 8px; padding: 16px; }

    Row {
        id: topbar;
        height: 48px;
        spacing: 16px;
        Label {
            id: device-title;
            width: 340px;
            height: 48px;
            text: t("STR-NS-TITLE");
            color: Theme.Colors.Title;
        }
        Clock {
            id: wall-clock;
            width: Fill;
            height: 48px;
            format: DateTimeSeconds;
        }
        StatusIndicator {
            id: system-status;
            width: 200px;
            height: 48px;
            requirement: "REQ-NS-003";
            source: "MONITOR_STATUS";
            states: [t("STR-NS-NOMINAL"), t("STR-NS-ALERT"), t("STR-NS-FAULT")];
        }
    }

    @safety_critical(cv_check: [Bounds, ColorHash])
    NumericDisplay {
        id: sedation-index;
        width: Fill;
        height: 120px;
        requirement: "REQ-NS-001";
        template: "TPL-SEDATION-INDEX";
        source: "SEDATION_INDEX";
        color: Theme.Colors.ScoreDigits;
    }

    VulkanViewport {
        id: eeg-dsa;
        width: Fill;
        height: Fill;
        stream_source: "EEG_DSA";
    }
}
"#;

    #[test]
    fn compiles_monitor_screen_with_row_to_flat_absolute_bounds() {
        let standard = monitor_text_package();
        let display = display_text_package();
        let generated = compile_medui_source_to_rust(
            MONITOR_MEDUI,
            CompileOptions::new(1280, 720),
            TextPackages::with_display(&standard, &display),
        )
        .expect("monitor screen should compile");

        // The Row disappears: only its children remain, at absolute positions.
        assert!(!generated.contains("topbar"));
        // Row children: title at padding, clock fills between, status flush right.
        assert!(generated.contains("id: \"device-title\""));
        assert!(generated.contains("bounds: ::mdux::Rect { x: 16, y: 16, width: 340, height: 48 }"));
        assert!(generated.contains("bounds: ::mdux::Rect { x: 372, y: 16, width: 676, height: 48 }"));
        assert!(generated.contains("bounds: ::mdux::Rect { x: 1064, y: 16, width: 200, height: 48 }"));
        // Vertical flow after the row: numeric display then viewport filling the rest.
        assert!(generated.contains("bounds: ::mdux::Rect { x: 16, y: 72, width: 1248, height: 120 }"));
        assert!(generated.contains("bounds: ::mdux::Rect { x: 16, y: 200, width: 1248, height: 504 }"));
        // Kinds emitted fully qualified.
        assert!(generated.contains("::mdux::CompiledNodeKind::Label(::mdux::LabelSpec"));
        assert!(generated.contains("::mdux::ClockFormat::DateTimeSeconds"));
        assert!(generated.contains(
            "state_text_keys: &[\"STR-NS-NOMINAL\", \"STR-NS-ALERT\", \"STR-NS-FAULT\"]"
        ));
        // Golden reference for the safety-critical numeric display: bounds + color, no text key.
        assert!(generated.contains("node_id: \"sedation-index\""));
        assert!(generated.contains("color_token: Some(\"Theme.Colors.ScoreDigits\")"));
    }

    #[test]
    fn rejects_status_state_wider_than_its_bounds() {
        let standard = monitor_text_package();
        let display = display_text_package();
        // Shrink the status indicator below the widest fr-FR state label (2 glyphs = 16px).
        let source = MONITOR_MEDUI.replace(
            "            width: 200px;\n            height: 48px;\n            requirement: \"REQ-NS-003\";",
            "            width: 15px;\n            height: 48px;\n            requirement: \"REQ-NS-003\";",
        );
        let error = compile_medui_source_to_rust(
            &source,
            CompileOptions::new(1280, 720),
            TextPackages::with_display(&standard, &display),
        )
        .expect_err("over-wide status state should be rejected");

        assert!(error.to_string().contains("exceeds bounds"), "{error}");
    }

    #[test]
    fn rejects_numeric_display_without_display_package() {
        let standard = monitor_text_package();
        let error = compile_medui_source_to_rust(
            MONITOR_MEDUI,
            CompileOptions::new(1280, 720),
            TextPackages::standard_only(&standard),
        )
        .expect_err("numeric display requires the display package");

        assert!(
            error
                .to_string()
                .contains("requires a display text package"),
            "{error}"
        );
    }

    #[test]
    fn rejects_numeric_display_narrower_than_its_worst_case_digits() {
        let standard = monitor_text_package();
        let display = display_text_package();
        // Worst case is 2 × 26px = 52px wide; a 240px-wide fixed screen leaves 240-32=208 for
        // content, so shrink via an explicit narrow width instead.
        let source = MONITOR_MEDUI.replace(
            "        id: sedation-index;\n        width: Fill;",
            "        id: sedation-index;\n        width: 51px;",
        );
        let error = compile_medui_source_to_rust(
            &source,
            CompileOptions::new(1280, 720),
            TextPackages::with_display(&standard, &display),
        )
        .expect_err("too-narrow numeric display should be rejected");

        assert!(error.to_string().contains("does not fit its bounds"), "{error}");
    }

    #[test]
    fn rejects_nested_row_containers() {
        let source = MONITOR_MEDUI.replace(
            "        Label {",
            "        Row {\n            id: inner;\n            height: 24px;\n        }\n        Label {",
        );
        let standard = monitor_text_package();
        let display = display_text_package();
        let error = compile_medui_source_to_rust(
            &source,
            CompileOptions::new(1280, 720),
            TextPackages::with_display(&standard, &display),
        )
        .expect_err("nested rows should be rejected");

        assert!(
            error.to_string().contains("nested Row containers are not supported"),
            "{error}"
        );
    }

    #[test]
    fn rejects_rows_in_horizontal_screen_layouts() {
        let source = MONITOR_MEDUI.replace(
            "layout: Vertical { spacing: 8px; padding: 16px; }",
            "layout: Horizontal { spacing: 8px; padding: 16px; }",
        );
        let standard = monitor_text_package();
        let display = display_text_package();
        let error = compile_medui_source_to_rust(
            &source,
            CompileOptions::new(1280, 720),
            TextPackages::with_display(&standard, &display),
        )
        .expect_err("rows require a vertical screen layout");

        assert!(
            error.to_string().contains("Row containers require a Vertical screen layout"),
            "{error}"
        );
    }

    #[test]
    fn rejects_clock_narrower_than_its_fixed_format() {
        // DateTimeSeconds needs 19 glyphs × 8px = 152px.
        let source = MONITOR_MEDUI.replace(
            "            id: wall-clock;\n            width: Fill;",
            "            id: wall-clock;\n            width: 151px;",
        );
        let standard = monitor_text_package();
        let display = display_text_package();
        let error = compile_medui_source_to_rust(
            &source,
            CompileOptions::new(1280, 720),
            TextPackages::with_display(&standard, &display),
        )
        .expect_err("too-narrow clock should be rejected");

        assert!(error.to_string().contains("does not fit its bounds"), "{error}");
    }

    #[test]
    fn status_indicator_without_colors_defaults_to_the_neutral_theme_token() {
        let standard = monitor_text_package();
        let display = display_text_package();
        let generated = compile_medui_source_to_rust(
            MONITOR_MEDUI,
            CompileOptions::new(1280, 720),
            TextPackages::with_display(&standard, &display),
        )
        .expect("monitor screen should compile");

        assert!(generated.contains("Theme.Colors.Neutral"), "{generated}");
        assert!(!generated.contains("Theme.Colors.StatusState"), "{generated}");
    }

    #[test]
    fn rejects_a_numeric_glyph_set_entry_with_no_matching_atlas_glyph() {
        let standard = monitor_text_package();
        let mut display = display_text_package();
        // Remove the widest digit's atlas glyph: the sedation-index budget check measures it,
        // and must fail loudly rather than silently under-measuring height.
        display.atlas_glyphs.retain(|glyph| glyph.glyph_id != 109);

        let error = compile_medui_source_to_rust(
            MONITOR_MEDUI,
            CompileOptions::new(1280, 720),
            TextPackages::with_display(&standard, &display),
        )
        .expect_err("a numeric glyph set entry with a dangling atlas reference should be rejected");

        assert!(error.to_string().contains("does not exist in the package"), "{error}");
    }

    /// ADR-014 positioning fixture: zero padding, a full-width topbar Row with a background
    /// and only positioned children, and two positioned top-level components filling the rest.
    /// Uses the same 8px-advance monitor_text_package/display_text_package fixtures.
    const POSITIONED_MEDUI: &str = r#"
Screen PositionedMonitor {
    layout: Vertical { spacing: 8px; padding: 0px; }
    surface: 1280px, 720px;
    Row {
        id: topbar;
        height: 48px;
        background: Theme.Colors.TopbarBackground;
        Label {
            id: device-title;
            width: 340px;
            height: 48px;
            position: 16px, 0px;
            text: t("STR-NS-TITLE");
            color: Theme.Colors.Title;
        }
        Clock {
            id: wall-clock;
            width: 448px;
            height: 48px;
            position: 372px, 0px;
            format: DateTimeSeconds;
        }
    }
    @safety_critical(cv_check: [Bounds, ColorHash])
    NumericDisplay {
        id: sedation-index;
        width: 512px;
        height: 512px;
        position: 752px, 56px;
        requirement: "REQ-NS-001";
        template: "TPL-SEDATION-INDEX";
        source: "SEDATION_INDEX";
        color: Theme.Colors.ScoreDigits;
    }
    VulkanViewport {
        id: eeg-dsa;
        width: 736px;
        height: 656px;
        position: 0px, 56px;
        stream_source: "EEG_DSA";
    }
}
"#;

    fn compile_positioned(source: &str) -> MduxResult<String> {
        let standard = monitor_text_package();
        let display = display_text_package();
        compile_medui_source_to_rust(
            source,
            CompileOptions::new(1280, 720),
            TextPackages::with_display(&standard, &display),
        )
    }

    #[test]
    fn compiles_positioned_screen_with_exact_declared_rects() {
        let generated = compile_positioned(POSITIONED_MEDUI)
            .expect("positioned monitor screen should compile");

        // The surface pin is emitted as the app's single source of truth.
        assert!(generated.contains("pub const GENERATED_MEDUI_SURFACE: (u32, u32) = (1280, 720);"));
        // Full-width Panel underlay synthesized from the Row background at padding 0.
        assert!(generated.contains(r#"id: "topbar-background""#));
        assert!(generated.contains(
            r#"::mdux::CompiledNodeKind::Panel(::mdux::PanelSpec { color_token: "Theme.Colors.TopbarBackground" })"#
        ));
        assert!(generated.contains("bounds: ::mdux::Rect { x: 0, y: 0, width: 1280, height: 48 }"));
        // Positioned nodes land at EXACTLY their declared coordinates.
        assert!(generated.contains("bounds: ::mdux::Rect { x: 16, y: 0, width: 340, height: 48 }"));
        assert!(generated.contains("bounds: ::mdux::Rect { x: 372, y: 0, width: 448, height: 48 }"));
        assert!(generated.contains("bounds: ::mdux::Rect { x: 752, y: 56, width: 512, height: 512 }"));
        assert!(generated.contains("bounds: ::mdux::Rect { x: 0, y: 56, width: 736, height: 656 }"));
    }

    #[test]
    fn positioned_nodes_get_automatic_bounds_golden_references() {
        let generated = compile_positioned(POSITIONED_MEDUI)
            .expect("positioned monitor screen should compile");

        // Four positioned nodes -> four golden entries; the Panel gets none.
        assert_eq!(generated.matches("::mdux::GoldenReferenceEntry {").count(), 4);
        assert!(!generated.contains(r#"node_id: "topbar-background""#));
        // Un-annotated positioned nodes are pinned with Bounds only.
        assert!(generated.contains(r#"node_id: "device-title""#));
        assert!(generated.contains(r#"node_id: "wall-clock""#));
        assert!(generated.contains(r#"node_id: "eeg-dsa""#));
        // The @safety_critical positioned node gets ONE merged entry (its own checks already
        // contain Bounds — no duplicate).
        assert_eq!(generated.matches(r#"node_id: "sedation-index""#).count(), 1);
        assert!(generated.contains(
            "cv_checks: &[::mdux::CvCheckKind::Bounds, ::mdux::CvCheckKind::ColorHash]"
        ));
    }

    #[test]
    fn positioned_component_rejects_widest_translation_growth() {
        // The epic's headline requirement: the title box is pinned at exactly the width of the
        // widest approved translation (fr-FR "ABC" = 3 glyphs x 8px = 24px). A translation
        // growing by a single pixel must fail the compile — the i18n alert.
        let fits = POSITIONED_MEDUI
            .replace("            width: 340px;", "            width: 24px;");
        compile_positioned(&fits).expect("box exactly as wide as the widest translation fits");

        let overflows = POSITIONED_MEDUI
            .replace("            width: 340px;", "            width: 23px;");
        let error = compile_positioned(&overflows)
            .expect_err("a pinned box narrower than the widest approved translation must fail");
        assert!(error.to_string().contains("exceeds bounds"), "{error}");
    }

    #[test]
    fn rejects_positioned_child_escaping_its_row() {
        // Push the clock 1px below the 48px row.
        let source = POSITIONED_MEDUI.replace(
            "            position: 372px, 0px;",
            "            position: 372px, 1px;",
        );
        let error = compile_positioned(&source).expect_err("escaping row child must fail");
        assert!(error.to_string().contains("escapes its Row topbar"), "{error}");
    }

    #[test]
    fn rejects_positioned_component_escaping_the_surface() {
        // 769 + 512 = 1281 > 1280.
        let source = POSITIONED_MEDUI.replace(
            "        position: 752px, 56px;",
            "        position: 769px, 56px;",
        );
        let error = compile_positioned(&source).expect_err("surface escape must fail");
        assert!(error.to_string().contains("exceeds the available surface"), "{error}");
    }

    #[test]
    fn rejects_overlapping_positioned_components() {
        // Slide the numeric display left onto the viewport (700 < 736 = viewport right edge).
        let source = POSITIONED_MEDUI.replace(
            "        position: 752px, 56px;",
            "        position: 700px, 56px;",
        );
        let error = compile_positioned(&source).expect_err("overlap must fail");
        assert!(
            error.to_string().contains("overlaps component"),
            "{error}"
        );

        // Positioned-vs-flow: raise the viewport into the topbar row's flow children.
        let source = POSITIONED_MEDUI.replace(
            "        position: 0px, 56px;",
            "        position: 0px, 40px;",
        );
        let error = compile_positioned(&source).expect_err("positioned-vs-flow overlap must fail");
        assert!(error.to_string().contains("overlaps component"), "{error}");
    }

    #[test]
    fn rejects_fill_with_position() {
        let source = POSITIONED_MEDUI.replace(
            "        width: 736px;",
            "        width: Fill;",
        );
        let error = compile_positioned(&source).expect_err("Fill + position must fail");
        assert!(
            error.to_string().contains("`position` requires fixed `width`/`height`"),
            "{error}"
        );
    }

    #[test]
    fn rejects_surface_pin_mismatch() {
        let source = POSITIONED_MEDUI.replace(
            "    surface: 1280px, 720px;",
            "    surface: 1920px, 1080px;",
        );
        let error = compile_positioned(&source).expect_err("surface pin mismatch must fail");
        assert!(
            error.to_string().contains("declares surface 1920x1080 but the build configured 1280x720"),
            "{error}"
        );
    }

    #[test]
    fn rejects_unknown_theme_color_token() {
        let source = POSITIONED_MEDUI.replace(
            "            color: Theme.Colors.Title;",
            "            color: Theme.Colors.Titel;",
        );
        let error = compile_positioned(&source).expect_err("unknown token must fail");
        assert!(
            error.to_string().contains("unknown theme color token `Theme.Colors.Titel`"),
            "{error}"
        );
        assert!(error.to_string().contains("Theme.Colors.Title"), "{error}");
    }

    #[test]
    fn rejects_a_user_id_colliding_with_a_synthesized_background_id() {
        let source = POSITIONED_MEDUI.replace(
            "        id: sedation-index;",
            "        id: topbar-background;",
        );
        let error = compile_positioned(&source).expect_err("duplicate node id must fail");
        assert!(
            error.to_string().contains("duplicate node id topbar-background"),
            "{error}"
        );
    }

    #[test]
    fn zero_is_legal_for_layout_but_not_for_component_sizes() {
        // POSITIONED_MEDUI already exercises padding: 0px and position: ...,0px — compiled above.
        let source = POSITIONED_MEDUI.replace(
            "            width: 340px;",
            "            width: 0px;",
        );
        let error = compile_positioned(&source).expect_err("zero component width must fail");
        assert!(error.to_string().contains("greater than zero"), "{error}");
    }

    /// Standard-package fixture for monitor screens: title and status strings in en-US and a
    /// wider fr-FR, plus the clock glyph set (digits, '-', ':', space) at 8px advances.
    fn monitor_text_package() -> TextPackage {
        let mut atlas_glyphs = vec![
            // Glyph 1: the generic 8x16 text glyph every run reuses.
            AtlasGlyph {
                atlas_index: 0,
                glyph_id: 1,
                x: 0,
                y: 0,
                width: 8,
                height: 16,
                bearing_x: 0,
                bearing_y: 0,
                advance_x: 8,
            },
        ];
        // Glyphs 30..42: digits 0-9, '-', ':' (8x16); glyph 43: space (zero-size, advance 8).
        for glyph_id in 30..=42u32 {
            atlas_glyphs.push(AtlasGlyph {
                atlas_index: 0,
                glyph_id,
                x: 0,
                y: 0,
                width: 8,
                height: 16,
                bearing_x: 0,
                bearing_y: 0,
                advance_x: 8,
            });
        }
        atlas_glyphs.push(AtlasGlyph {
            atlas_index: 0,
            glyph_id: 43,
            x: 0,
            y: 0,
            width: 0,
            height: 0,
            bearing_x: 0,
            bearing_y: 0,
            advance_x: 8,
        });

        let string = |id: &str, locale: &str, value: &str| ApprovedString {
            id: id.to_string(),
            locale: locale.to_string(),
            value: value.to_string(),
            direction: TextDirection::LeftToRight,
        };
        let run = |id: &str, source: &str, locale: &str, glyph_count: usize| CompiledTextRun {
            id: id.to_string(),
            source_string_id: source.to_string(),
            locale: locale.to_string(),
            bidi_level: 0,
            glyphs: (0..glyph_count)
                .map(|index| CompiledGlyph {
                    atlas_index: 0,
                    glyph_id: 1,
                    x: index as i32 * 8,
                    y: 0,
                    advance_x: 8,
                })
                .collect(),
        };

        let mut entries = Vec::new();
        for (offset, character) in "0123456789-:".chars().enumerate() {
            entries.push(mdux_text_schema::NumericGlyphEntry {
                character,
                glyph_id: 30 + offset as u32,
                atlas_index: 0,
                advance_x: 8,
            });
        }
        entries.push(mdux_text_schema::NumericGlyphEntry {
            character: ' ',
            glyph_id: 43,
            atlas_index: 0,
            advance_x: 8,
        });

        TextPackage {
            fonts: vec![FontAsset {
                family: "Approved Sans".to_string(),
                source_path: "fonts/approved.ttf".to_string(),
                sha256: "0123456789abcdef0123456789abcdef0123456789abcdef0123456789abcdef"
                    .to_string(),
                face_index: 0,
                pixel_height: 16,
                locales: vec!["en-US".to_string(), "fr-FR".to_string()],
            }],
            approved_strings: vec![
                string("STR-NS-TITLE", "en-US", "AB"),
                string("STR-NS-TITLE", "fr-FR", "ABC"),
                string("STR-NS-NOMINAL", "en-US", "N"),
                string("STR-NS-NOMINAL", "fr-FR", "N"),
                string("STR-NS-ALERT", "en-US", "A"),
                string("STR-NS-ALERT", "fr-FR", "AL"),
                string("STR-NS-FAULT", "en-US", "F"),
                string("STR-NS-FAULT", "fr-FR", "FD"),
            ],
            atlases: vec![TextureAtlas {
                width: 4,
                height: 4,
                pixels: vec![1; 16],
            }],
            atlas_glyphs,
            runs: vec![
                run("RUN-NS-TITLE-EN", "STR-NS-TITLE", "en-US", 2),
                run("RUN-NS-TITLE-FR", "STR-NS-TITLE", "fr-FR", 3),
                run("RUN-NS-NOMINAL-EN", "STR-NS-NOMINAL", "en-US", 1),
                run("RUN-NS-NOMINAL-FR", "STR-NS-NOMINAL", "fr-FR", 1),
                run("RUN-NS-ALERT-EN", "STR-NS-ALERT", "en-US", 1),
                run("RUN-NS-ALERT-FR", "STR-NS-ALERT", "fr-FR", 2),
                run("RUN-NS-FAULT-EN", "STR-NS-FAULT", "en-US", 1),
                run("RUN-NS-FAULT-FR", "STR-NS-FAULT", "fr-FR", 2),
            ],
            numeric_glyph_sets: vec![NumericGlyphSet {
                id: CLOCK_GLYPH_SET_ID.to_string(),
                locale: "en-US".to_string(),
                entries,
            }],
            numeric_templates: vec![],
            evidence: DeterminismEvidence {
                package_sha256:
                    "0123456789abcdef0123456789abcdef0123456789abcdef0123456789abcdef"
                        .to_string(),
                toolchain_id: "rust-1.87.0".to_string(),
                unicode_version: "15.1.0".to_string(),
                build_recipe_sha256:
                    "fedcba9876543210fedcba9876543210fedcba9876543210fedcba9876543210"
                        .to_string(),
            },
        }
    }

    /// Display-package fixture: 48px digits (26px advances) and the affixless sedation template.
    fn display_text_package() -> TextPackage {
        let mut atlas_glyphs = Vec::new();
        let mut entries = Vec::new();
        for (offset, character) in "0123456789".chars().enumerate() {
            let glyph_id = 100 + offset as u32;
            atlas_glyphs.push(AtlasGlyph {
                atlas_index: 0,
                glyph_id,
                x: 0,
                y: 0,
                width: 24,
                height: 48,
                bearing_x: 0,
                bearing_y: 0,
                advance_x: 26,
            });
            entries.push(mdux_text_schema::NumericGlyphEntry {
                character,
                glyph_id,
                atlas_index: 0,
                advance_x: 26,
            });
        }

        TextPackage {
            fonts: vec![FontAsset {
                family: "Approved Sans".to_string(),
                source_path: "fonts/approved.ttf".to_string(),
                sha256: "0123456789abcdef0123456789abcdef0123456789abcdef0123456789abcdef"
                    .to_string(),
                face_index: 0,
                pixel_height: 48,
                locales: vec!["en-US".to_string()],
            }],
            approved_strings: vec![ApprovedString {
                id: "STR-DISPLAY-DIGITS".to_string(),
                locale: "en-US".to_string(),
                value: "0123456789".to_string(),
                direction: TextDirection::LeftToRight,
            }],
            atlases: vec![TextureAtlas {
                width: 4,
                height: 4,
                pixels: vec![1; 16],
            }],
            atlas_glyphs,
            runs: vec![CompiledTextRun {
                id: "RUN-DISPLAY-DIGITS".to_string(),
                source_string_id: "STR-DISPLAY-DIGITS".to_string(),
                locale: "en-US".to_string(),
                bidi_level: 0,
                glyphs: vec![CompiledGlyph {
                    atlas_index: 0,
                    glyph_id: 100,
                    x: 0,
                    y: 0,
                    advance_x: 26,
                }],
            }],
            numeric_glyph_sets: vec![NumericGlyphSet {
                id: "SET-DISPLAY-DIGITS-48".to_string(),
                locale: "en-US".to_string(),
                entries,
            }],
            numeric_templates: vec![mdux_text_schema::NumericTemplate {
                id: "TPL-SEDATION-INDEX".to_string(),
                locale: "en-US".to_string(),
                prefix_run_id: None,
                suffix_run_id: None,
                glyph_set_id: "SET-DISPLAY-DIGITS-48".to_string(),
                max_chars: 2,
                allow_negative: false,
            }],
            evidence: DeterminismEvidence {
                package_sha256:
                    "0123456789abcdef0123456789abcdef0123456789abcdef0123456789abcdef"
                        .to_string(),
                toolchain_id: "rust-1.87.0".to_string(),
                unicode_version: "15.1.0".to_string(),
                build_recipe_sha256:
                    "fedcba9876543210fedcba9876543210fedcba9876543210fedcba9876543210"
                        .to_string(),
            },
        }
    }

    fn sample_text_package() -> TextPackage {
        TextPackage {
            fonts: vec![FontAsset {
                family: "Approved Sans".to_string(),
                source_path: "fonts/approved.ttf".to_string(),
                sha256: "0123456789abcdef0123456789abcdef0123456789abcdef0123456789abcdef"
                    .to_string(),
                face_index: 0,
                pixel_height: 16,
                locales: vec!["en-US".to_string(), "fr-FR".to_string()],
            }],
            approved_strings: vec![
                ApprovedString {
                    id: "STR-HELLO-WORLD".to_string(),
                    locale: "en-US".to_string(),
                    value: "Hello World!".to_string(),
                    direction: TextDirection::LeftToRight,
                },
                ApprovedString {
                    id: "STR-HELLO-WORLD".to_string(),
                    locale: "fr-FR".to_string(),
                    value: "Bonjour monde!".to_string(),
                    direction: TextDirection::LeftToRight,
                },
            ],
            atlases: vec![TextureAtlas {
                width: 4,
                height: 4,
                pixels: vec![1; 16],
            }],
            atlas_glyphs: vec![
                AtlasGlyph {
                    atlas_index: 0,
                    glyph_id: 1,
                    x: 0,
                    y: 0,
                    width: 56,
                    height: 16,
                    bearing_x: 0,
                    bearing_y: 0,
                    advance_x: 56,
                },
                AtlasGlyph {
                    atlas_index: 0,
                    glyph_id: 2,
                    x: 0,
                    y: 0,
                    width: 96,
                    height: 16,
                    bearing_x: 0,
                    bearing_y: 0,
                    advance_x: 96,
                },
            ],
            runs: vec![
                CompiledTextRun {
                    id: "RUN-HELLO-EN".to_string(),
                    source_string_id: "STR-HELLO-WORLD".to_string(),
                    locale: "en-US".to_string(),
                    bidi_level: 0,
                    glyphs: vec![CompiledGlyph {
                        atlas_index: 0,
                        glyph_id: 1,
                        x: 0,
                        y: 0,
                        advance_x: 56,
                    }],
                },
                CompiledTextRun {
                    id: "RUN-HELLO-FR".to_string(),
                    source_string_id: "STR-HELLO-WORLD".to_string(),
                    locale: "fr-FR".to_string(),
                    bidi_level: 0,
                    glyphs: vec![CompiledGlyph {
                        atlas_index: 0,
                        glyph_id: 2,
                        x: 0,
                        y: 0,
                        advance_x: 96,
                    }],
                },
            ],
            numeric_glyph_sets: vec![],
            numeric_templates: vec![],
            evidence: DeterminismEvidence {
                package_sha256:
                    "0123456789abcdef0123456789abcdef0123456789abcdef0123456789abcdef"
                        .to_string(),
                toolchain_id: "rust-1.87.0".to_string(),
                unicode_version: "15.1.0".to_string(),
                build_recipe_sha256:
                    "fedcba9876543210fedcba9876543210fedcba9876543210fedcba9876543210"
                        .to_string(),
            },
        }
    }
}
