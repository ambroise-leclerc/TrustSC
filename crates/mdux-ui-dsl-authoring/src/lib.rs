#![forbid(unsafe_code)]

use std::{
    fmt::Write as _,
    fs,
    path::Path,
};

use mdux_core::{MduxResult, ValidationError};
use mdux_text_schema::{CompiledTextRun, TextPackage};
use mdux_ui::{CvCheckKind, LayoutKind, SystemEvent};

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct CompileOptions {
    pub surface_width: u32,
    pub surface_height: u32,
}

impl CompileOptions {
    pub const fn new(surface_width: u32, surface_height: u32) -> Self {
        Self {
            surface_width,
            surface_height,
        }
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
    nodes: Vec<NodeDefinition>,
}

#[derive(Clone, Debug, Eq, PartialEq)]
struct NodeDefinition {
    id: String,
    width: Dimension,
    height: Dimension,
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
    nodes: Vec<CompiledNodeSpec>,
    golden_references: Vec<GoldenReferenceSpec>,
}

pub fn compile_medui_file_to_rust_module(
    input_path: impl AsRef<Path>,
    output_path: impl AsRef<Path>,
    options: CompileOptions,
    text_package: &TextPackage,
) -> MduxResult<()> {
    let input_path = input_path.as_ref();
    let output_path = output_path.as_ref();
    let source = fs::read_to_string(input_path).map_err(|error| {
        ValidationError::new(format!(
            "failed to read MedUI source {}: {error}",
            input_path.display()
        ))
    })?;
    let generated = compile_medui_source_to_rust(&source, options, text_package)?;

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
    text_package: &TextPackage,
) -> MduxResult<String> {
    let parsed = parse_screen(source)?;
    let compiled = compile_screen(parsed, options, text_package)?;
    Ok(emit_rust_module(&compiled))
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
    let mut nodes = Vec::new();
    let mut pending_safety: Option<SafetyCriticalDefinition> = None;
    let mut cursor = 2usize;

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
        nodes.push(node);
    }

    if nodes.is_empty() {
        return Err(ValidationError::new(
            "MedUI screen must declare at least one component",
        ));
    }

    Ok(ScreenDefinition {
        id: screen_id,
        layout,
        nodes,
    })
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
            parse_px(line_number, key, value.trim())
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
}

fn parse_component_start(line_number: usize, line: &str) -> MduxResult<ComponentKind> {
    let kind = line
        .strip_suffix('{')
        .map(str::trim)
        .ok_or_else(|| ValidationError::new(format!("expected component block at line {line_number}")))?;
    match kind {
        "CriticalButton" => Ok(ComponentKind::CriticalButton),
        "VulkanViewport" => Ok(ComponentKind::VulkanViewport),
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
            "color" => color_token = Some(parse_non_empty(*property_line_number, "color", value)?),
            "on_press" => on_press = Some(parse_system_event(*property_line_number, value)?),
            "stream_source" => {
                stream_source = Some(parse_quoted(*property_line_number, "stream_source", value)?)
            }
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
    };

    Ok(NodeDefinition {
        id,
        width,
        height,
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

fn parse_dimension(line_number: usize, field_name: &str, raw: &str) -> MduxResult<Dimension> {
    match raw.trim() {
        "Fill" => Ok(Dimension::Fill),
        other => parse_px(line_number, field_name, other).map(|value| Dimension::Px(u32::from(value))),
    }
}

fn parse_px(line_number: usize, field_name: &str, raw: &str) -> MduxResult<u16> {
    let px_value = raw
        .trim()
        .strip_suffix("px")
        .ok_or_else(|| {
            ValidationError::new(format!(
                "{field_name} must use a px unit at line {line_number}"
            ))
        })?
        .parse::<u16>()
        .map_err(|_| {
            ValidationError::new(format!(
                "{field_name} must be a positive integer px value at line {line_number}"
            ))
        })?;

    if px_value == 0 {
        return Err(ValidationError::new(format!(
            "{field_name} must be greater than zero at line {line_number}"
        )));
    }

    Ok(px_value)
}

fn compile_screen(
    screen: ScreenDefinition,
    options: CompileOptions,
    text_package: &TextPackage,
) -> MduxResult<CompiledScreenSpec> {
    if options.surface_width == 0 || options.surface_height == 0 {
        return Err(ValidationError::new(
            "compile options surface dimensions must be greater than zero",
        ));
    }

    let content_width = options
        .surface_width
        .checked_sub(u32::from(screen.layout.padding) * 2)
        .ok_or_else(|| ValidationError::new("layout padding exceeds surface width"))?;
    let content_height = options
        .surface_height
        .checked_sub(u32::from(screen.layout.padding) * 2)
        .ok_or_else(|| ValidationError::new("layout padding exceeds surface height"))?;

    let resolved_widths = resolve_axis_sizes(
        screen.nodes.iter().map(|node| node.width),
        if screen.layout.kind == LayoutKind::Horizontal {
            content_width
        } else {
            0
        },
        usize::from(screen.layout.spacing),
    )?;
    let resolved_heights = resolve_axis_sizes(
        screen.nodes.iter().map(|node| node.height),
        if screen.layout.kind == LayoutKind::Vertical {
            content_height
        } else {
            0
        },
        usize::from(screen.layout.spacing),
    )?;

    let mut cursor_x = i32::from(screen.layout.padding);
    let mut cursor_y = i32::from(screen.layout.padding);
    let mut nodes = Vec::with_capacity(screen.nodes.len());
    let mut golden_references = Vec::new();
    let screen_id = screen.id.clone();
    let layout = screen.layout.clone();
    let spacing = i32::from(layout.spacing);
    let padding = i32::from(layout.padding);

    for (index, node) in screen.nodes.into_iter().enumerate() {
        let width = match node.width {
            Dimension::Fill if layout.kind == LayoutKind::Vertical => content_width,
            Dimension::Fill => resolved_widths[index],
            Dimension::Px(value) => u32::from(value),
        };
        let height = match node.height {
            Dimension::Fill if layout.kind == LayoutKind::Horizontal => content_height,
            Dimension::Fill => resolved_heights[index],
            Dimension::Px(value) => u32::from(value),
        };

        let bounds = RectSpec {
            x: cursor_x,
            y: cursor_y,
            width,
            height,
        };

        if bounds.x < padding || bounds.y < padding {
            return Err(ValidationError::new(format!(
                "component {} resolved outside the padded surface",
                node.id
            )));
        }
        if bounds.x + bounds.width as i32 > options.surface_width as i32 - padding
            || bounds.y + bounds.height as i32 > options.surface_height as i32 - padding
        {
            return Err(ValidationError::new(format!(
                "component {} exceeds the available surface",
                node.id
            )));
        }

        validate_node_text_budget(&node, bounds, text_package)?;

        let compiled_node = CompiledNodeSpec {
            id: node.id.clone(),
            bounds,
            kind: node.kind.clone(),
        };

        if let Some(safety) = node.safety_critical {
            let (text_key, color_token) = match &node.kind {
                NodeKind::CriticalButton {
                    label_text_key,
                    color_token,
                    ..
                } => (Some(label_text_key.clone()), Some(color_token.clone())),
                NodeKind::VulkanViewport { .. } => (None, None),
            };
            golden_references.push(GoldenReferenceSpec {
                node_id: node.id.clone(),
                bounds,
                text_key,
                color_token,
                cv_checks: safety.cv_checks,
            });
        }

        nodes.push(compiled_node);

        match layout.kind {
            LayoutKind::Vertical => cursor_y += bounds.height as i32 + spacing,
            LayoutKind::Horizontal => cursor_x += bounds.width as i32 + spacing,
        }
    }

    Ok(CompiledScreenSpec {
        id: screen_id,
        layout,
        nodes,
        golden_references,
    })
}

fn validate_node_text_budget(
    node: &NodeDefinition,
    bounds: RectSpec,
    text_package: &TextPackage,
) -> MduxResult<()> {
    let NodeKind::CriticalButton { label_text_key, .. } = &node.kind else {
        return Ok(());
    };

    let locales = text_package
        .approved_strings
        .iter()
        .filter(|approved_string| approved_string.id == *label_text_key)
        .map(|approved_string| approved_string.locale.as_str())
        .collect::<Vec<_>>();

    if locales.is_empty() {
        return Err(ValidationError::new(format!(
            "text key {label_text_key} for component {} does not exist in the approved text package",
            node.id
        )));
    }

    for locale in locales {
        let run = text_package
            .find_run_for_string(label_text_key, locale)
            .ok_or_else(|| {
                ValidationError::new(format!(
                    "text key {label_text_key} for component {} is missing a compiled run for locale {locale}",
                    node.id
                ))
            })?;
        let run_bounds = measure_text_run_bounds(text_package, run)?;
        if run_bounds.width() > bounds.width || run_bounds.height() > bounds.height {
            return Err(ValidationError::new(format!(
                "text key {label_text_key} for component {} exceeds bounds in locale {locale}: required width={} height={}, available width={} height={}",
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

fn emit_rust_module(compiled: &CompiledScreenSpec) -> String {
    let mut output = String::new();
    let _ = writeln!(output, "pub const GENERATED_PRIMARY_TEXT_NODE_ID: &str = {:?};", compiled
        .nodes
        .iter()
        .find_map(|node| matches!(node.kind, NodeKind::CriticalButton { .. }).then_some(node.id.as_str()))
        .unwrap_or(""));
    let _ = writeln!(
        output,
        "pub const GENERATED_MEDUI_PACKAGE: CompiledScreenPackage = CompiledScreenPackage {{"
    );
    let _ = writeln!(output, "    screen_id: {:?},", compiled.id);
    let _ = writeln!(
        output,
        "    layout: LayoutSpec {{ kind: {}, spacing: {}, padding: {} }},",
        emit_layout_kind(compiled.layout.kind),
        compiled.layout.spacing,
        compiled.layout.padding
    );
    let _ = writeln!(output, "    nodes: &[");
    for node in &compiled.nodes {
        let _ = writeln!(output, "        CompiledNode {{");
        let _ = writeln!(output, "            id: {:?},", node.id);
        let _ = writeln!(
            output,
            "            bounds: Rect {{ x: {}, y: {}, width: {}, height: {} }},",
            node.bounds.x, node.bounds.y, node.bounds.width, node.bounds.height
        );
        let _ = writeln!(output, "            kind: {},", emit_node_kind(&node.kind));
        let _ = writeln!(output, "        }},");
    }
    let _ = writeln!(output, "    ],");
    let _ = writeln!(output, "    golden_references: &[");
    for golden_reference in &compiled.golden_references {
        let _ = writeln!(output, "        GoldenReferenceEntry {{");
        let _ = writeln!(output, "            node_id: {:?},", golden_reference.node_id);
        let _ = writeln!(
            output,
            "            bounds: Rect {{ x: {}, y: {}, width: {}, height: {} }},",
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
            emit_cv_checks(&golden_reference.cv_checks)
        );
        let _ = writeln!(output, "        }},");
    }
    let _ = writeln!(output, "    ],");
    let _ = writeln!(output, "}};");
    output
}

fn emit_layout_kind(kind: LayoutKind) -> &'static str {
    match kind {
        LayoutKind::Vertical => "LayoutKind::Vertical",
        LayoutKind::Horizontal => "LayoutKind::Horizontal",
    }
}

fn emit_node_kind(kind: &NodeKind) -> String {
    match kind {
        NodeKind::CriticalButton {
            requirement_id,
            label_text_key,
            color_token,
            on_press,
        } => format!(
            "CompiledNodeKind::CriticalButton(CriticalButtonSpec {{ requirement_id: {requirement_id:?}, text_key: {label_text_key:?}, color_token: {color_token:?}, on_press: {} }})",
            emit_system_event(*on_press)
        ),
        NodeKind::VulkanViewport { stream_source } => format!(
            "CompiledNodeKind::VulkanViewport(ViewportReservation {{ stream_source: {stream_source:?} }})"
        ),
    }
}

fn emit_system_event(event: SystemEvent) -> &'static str {
    match event {
        SystemEvent::NoOp => "SystemEvent::NoOp",
        SystemEvent::TriggerHalt => "SystemEvent::TriggerHalt",
    }
}

fn emit_optional_string(value: Option<&str>) -> String {
    value
        .map(|entry| format!("Some({entry:?})"))
        .unwrap_or_else(|| "None".to_string())
}

fn emit_cv_checks(checks: &[CvCheckKind]) -> String {
    if checks.is_empty() {
        "&[]".to_string()
    } else {
        format!(
            "&[{}]",
            checks
                .iter()
                .map(|check| match check {
                    CvCheckKind::Bounds => "CvCheckKind::Bounds",
                    CvCheckKind::ColorHash => "CvCheckKind::ColorHash",
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
            &sample_text_package(),
        )
        .expect("sample medui should compile");

        assert!(generated.contains("pub const GENERATED_MEDUI_PACKAGE"));
        assert!(generated.contains("screen_id: \"HelloWorld\""));
        assert!(generated.contains("id: \"hello-world-label\""));
        assert!(generated.contains("stream_source: \"HELLO_WORLD_SIM\""));
        assert!(generated.contains("cv_checks: &[CvCheckKind::Bounds, CvCheckKind::ColorHash]"));
    }

    #[test]
    fn rejects_missing_requirement_binding() {
        let source = SAMPLE_MEDUI.replace("        requirement: \"REQ-HELLO-001\";\n", "");
        let error = compile_medui_source_to_rust(
            &source,
            CompileOptions::new(800, 480),
            &sample_text_package(),
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
            &sample_text_package(),
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
            &sample_text_package(),
        )
        .expect_err("widest translation should be rejected at compile time");

        assert_eq!(
            error.to_string(),
            "text key STR-HELLO-WORLD for component hello-world-label exceeds bounds in locale fr-FR: required width=96 height=16, available width=80 height=80"
        );
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
