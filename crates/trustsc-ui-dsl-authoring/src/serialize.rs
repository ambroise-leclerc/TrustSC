//! Canonical `.medui` serializer: `ScreenDefinition -> String`, the inverse of
//! [`crate::parse_medui_source`]. This is what lets a GUI (MedUI Studio, ADR-022) save an edited
//! screen as clean, diffable `.medui` text.
//!
//! **v1 limitation**: comments and blank lines are never preserved. The parser strips both as
//! trivia while tokenizing (`parse_screen`/`parse_row` only ever see trimmed, non-empty,
//! non-`//`-prefixed lines) and the AST has no trivia slots to carry them through — a save
//! round-trip is semantically faithful but not textually faithful to hand-added formatting or
//! comments. Trivia preservation is left to a later wave.
//!
//! Property order per component kind is a chosen canonical order (`id, width, height, position,`
//! then kind-specific properties), not a requirement of the parser: `parse_component_properties`
//! matches each `key: value;` line independently of position, so any order parses identically.
//! Picking one fixed order per kind is what makes saved output a single-line diff instead of a
//! shuffle.

use crate::{
    ASCII_TEXT_GLYPH_SET_ID, Dimension, LayoutDefinition, NodeDefinition, NodeKind, RowDefinition,
    SafetyCriticalDefinition, ScreenDefinition, ScreenItem,
};
use std::fmt::Write as _;
use trustsc_ui::{ClockFormat, CvCheckKind, LayoutKind, SystemEvent};

/// Serializes a [`ScreenDefinition`] to canonical `.medui` source text.
pub fn serialize_screen(screen: &ScreenDefinition) -> String {
    let mut out = String::new();
    let _ = writeln!(out, "Screen {} {{", screen.id);
    let _ = writeln!(out, "    {}", serialize_layout(&screen.layout));
    if let Some((width, height)) = screen.declared_surface {
        let _ = writeln!(out, "    surface: {width}px, {height}px;");
    }
    for item in &screen.items {
        match item {
            ScreenItem::Component(node) => serialize_component(&mut out, node, 4),
            ScreenItem::Row(row) => serialize_row(&mut out, row),
        }
    }
    let _ = writeln!(out, "}}");
    out
}

fn serialize_layout(layout: &LayoutDefinition) -> String {
    format!(
        "layout: {} {{ spacing: {}px; padding: {}px; }}",
        serialize_layout_kind(layout.kind),
        layout.spacing,
        layout.padding
    )
}

fn serialize_layout_kind(kind: LayoutKind) -> &'static str {
    match kind {
        LayoutKind::Vertical => "Vertical",
        LayoutKind::Horizontal => "Horizontal",
    }
}

fn serialize_row(out: &mut String, row: &RowDefinition) {
    let _ = writeln!(out, "    Row {{");
    let _ = writeln!(out, "        id: {};", row.id);
    let _ = writeln!(out, "        height: {};", serialize_dimension(row.height));
    let _ = writeln!(out, "        spacing: {}px;", row.spacing);
    if let Some(background) = &row.background {
        let _ = writeln!(out, "        background: {background};");
    }
    for child in &row.children {
        serialize_component(out, child, 8);
    }
    let _ = writeln!(out, "    }}");
}

fn serialize_component(out: &mut String, node: &NodeDefinition, indent: usize) {
    let pad = " ".repeat(indent);
    let inner_pad = " ".repeat(indent + 4);

    if let Some(safety) = &node.safety_critical {
        let _ = writeln!(out, "{pad}{}", serialize_safety_critical(safety));
    }
    let _ = writeln!(out, "{pad}{} {{", node_kind_name(&node.kind));
    let _ = writeln!(out, "{inner_pad}id: {};", node.id);
    let _ = writeln!(out, "{inner_pad}width: {};", serialize_dimension(node.width));
    let _ = writeln!(out, "{inner_pad}height: {};", serialize_dimension(node.height));
    if let Some((x, y)) = node.position {
        let _ = writeln!(out, "{inner_pad}position: {x}px, {y}px;");
    }
    serialize_node_kind_properties(out, &node.kind, &inner_pad);
    let _ = writeln!(out, "{pad}}}");
}

fn serialize_safety_critical(safety: &SafetyCriticalDefinition) -> String {
    let checks = safety
        .cv_checks
        .iter()
        .map(|check| match check {
            CvCheckKind::Bounds => "Bounds",
            CvCheckKind::ColorHash => "ColorHash",
        })
        .collect::<Vec<_>>()
        .join(", ");
    format!("@safety_critical(cv_check: [{checks}])")
}

fn node_kind_name(kind: &NodeKind) -> &'static str {
    match kind {
        NodeKind::CriticalButton { .. } => "CriticalButton",
        NodeKind::VulkanViewport { .. } => "VulkanViewport",
        NodeKind::SignalTrace { .. } => "SignalTrace",
        NodeKind::Label { .. } => "Label",
        NodeKind::Clock { .. } => "Clock",
        NodeKind::NumericDisplay { .. } => "NumericDisplay",
        NodeKind::StatusIndicator { .. } => "StatusIndicator",
        // Panel is synthesized by the compiler from a Row's `background:` token (ADR-014 flat
        // emission) and has no `.medui` component syntax of its own. `NodeKind` is public, so a
        // caller can construct one directly — this is an invalid-input panic, not an internal
        // invariant, hence the explicit `panic!` rather than `unreachable!`.
        NodeKind::Panel { .. } => {
            panic!("cannot serialize a Panel node: Panel is compiler-synthesized only and has no `.medui` component syntax")
        }
        NodeKind::Image { .. } => "Image",
        NodeKind::Button { .. } => "Button",
        NodeKind::TextInput { .. } => "TextInput",
    }
}

fn serialize_dimension(dimension: Dimension) -> String {
    match dimension {
        Dimension::Fill => "Fill".to_string(),
        Dimension::Px(value) => format!("{value}px"),
    }
}

fn serialize_system_event(event: SystemEvent) -> &'static str {
    match event {
        SystemEvent::NoOp => "SystemEvent.NoOp",
        SystemEvent::TriggerHalt => "SystemEvent.TriggerHalt",
    }
}

fn serialize_clock_format(format: ClockFormat) -> &'static str {
    match format {
        ClockFormat::TimeSeconds => "TimeSeconds",
        ClockFormat::DateTimeSeconds => "DateTimeSeconds",
    }
}

fn serialize_text_key(key: &str) -> String {
    format!("t(\"{key}\")")
}

fn serialize_text_key_list(keys: &[String]) -> String {
    let entries = keys
        .iter()
        .map(|key| serialize_text_key(key))
        .collect::<Vec<_>>()
        .join(", ");
    format!("[{entries}]")
}

fn serialize_token_list(tokens: &[String]) -> String {
    format!("[{}]", tokens.join(", "))
}

fn serialize_node_kind_properties(out: &mut String, kind: &NodeKind, pad: &str) {
    match kind {
        NodeKind::CriticalButton {
            requirement_id,
            label_text_key,
            color_token,
            on_press,
        } => {
            let _ = writeln!(out, "{pad}requirement: \"{requirement_id}\";");
            let _ = writeln!(out, "{pad}label: {};", serialize_text_key(label_text_key));
            let _ = writeln!(out, "{pad}color: {color_token};");
            let _ = writeln!(out, "{pad}on_press: {};", serialize_system_event(*on_press));
        }
        NodeKind::VulkanViewport { stream_source } => {
            let _ = writeln!(out, "{pad}stream_source: \"{stream_source}\";");
        }
        NodeKind::SignalTrace {
            stream_source,
            color_token,
        } => {
            let _ = writeln!(out, "{pad}stream_source: \"{stream_source}\";");
            let _ = writeln!(out, "{pad}color: {color_token};");
        }
        NodeKind::Label {
            text_key,
            color_token,
        } => {
            let _ = writeln!(out, "{pad}text: {};", serialize_text_key(text_key));
            let _ = writeln!(out, "{pad}color: {color_token};");
        }
        NodeKind::Clock { format } => {
            let _ = writeln!(out, "{pad}format: {};", serialize_clock_format(*format));
        }
        NodeKind::NumericDisplay {
            requirement_id,
            template_id,
            source,
            color_token,
        } => {
            let _ = writeln!(out, "{pad}requirement: \"{requirement_id}\";");
            let _ = writeln!(out, "{pad}template: \"{template_id}\";");
            let _ = writeln!(out, "{pad}source: \"{source}\";");
            let _ = writeln!(out, "{pad}color: {color_token};");
        }
        NodeKind::StatusIndicator {
            requirement_id,
            source,
            state_text_keys,
            color_tokens,
        } => {
            let _ = writeln!(out, "{pad}requirement: \"{requirement_id}\";");
            let _ = writeln!(out, "{pad}source: \"{source}\";");
            let _ = writeln!(out, "{pad}states: {};", serialize_text_key_list(state_text_keys));
            let _ = writeln!(out, "{pad}colors: {};", serialize_token_list(color_tokens));
        }
        NodeKind::Panel { .. } => {
            panic!("cannot serialize a Panel node: Panel is compiler-synthesized only and has no `.medui` component syntax")
        }
        NodeKind::Image { image_id } => {
            let _ = writeln!(out, "{pad}source: img(\"{image_id}\");");
        }
        NodeKind::Button {
            label_text_key,
            color_token,
            source,
            requirement_id,
        } => {
            if let Some(requirement_id) = requirement_id {
                let _ = writeln!(out, "{pad}requirement: \"{requirement_id}\";");
            }
            let _ = writeln!(out, "{pad}label: {};", serialize_text_key(label_text_key));
            let _ = writeln!(out, "{pad}color: {color_token};");
            let _ = writeln!(out, "{pad}source: \"{source}\";");
        }
        NodeKind::TextInput {
            source,
            max_length,
            glyph_set_id,
            color_token,
            requirement_id,
        } => {
            if let Some(requirement_id) = requirement_id {
                let _ = writeln!(out, "{pad}requirement: \"{requirement_id}\";");
            }
            let _ = writeln!(out, "{pad}source: \"{source}\";");
            let _ = writeln!(out, "{pad}max_length: {max_length};");
            // `AsciiText` is the only approved charset (`parse_charset`), so it is the only
            // `charset:` spelling that can reparse to this glyph set. `NodeKind` is public, so a
            // caller could hand us some other `glyph_set_id` directly — fail loudly instead of
            // silently coercing it to `AsciiText` on save.
            assert_eq!(
                glyph_set_id, ASCII_TEXT_GLYPH_SET_ID,
                "cannot serialize TextInput: glyph_set_id `{glyph_set_id}` has no `charset:` spelling (only `AsciiText` / `{ASCII_TEXT_GLYPH_SET_ID}` is approved)"
            );
            let _ = writeln!(out, "{pad}charset: AsciiText;");
            let _ = writeln!(out, "{pad}color: {color_token};");
        }
    }
}
