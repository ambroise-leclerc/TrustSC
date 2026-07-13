//! The `.medui` abstract syntax tree — owned data produced by [`crate::parse_medui_source`].
//!
//! This is the authoring-side representation of a screen: everything a GUI needs to inspect or
//! rewrite a design (MedUI Studio, ADR-022) without re-parsing generated Rust text. All types are
//! plain owned data (`String`/`Vec`-based) with `Clone, Debug, Eq, PartialEq` so a tool can diff,
//! hash, or round-trip them freely.

use trustsc_ui::{ClockFormat, CvCheckKind, LayoutKind, SystemEvent};

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum Dimension {
    Px(u32),
    Fill,
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct LayoutDefinition {
    pub kind: LayoutKind,
    pub spacing: u16,
    pub padding: u16,
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct SafetyCriticalDefinition {
    pub cv_checks: Vec<CvCheckKind>,
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct ScreenDefinition {
    pub id: String,
    pub layout: LayoutDefinition,
    /// Optional `surface: WxH` pin (ADR-014): compile fails if it disagrees with the build's
    /// configured surface.
    pub declared_surface: Option<(u32, u32)>,
    pub items: Vec<ScreenItem>,
}

/// A top-level entry in the screen flow: either a leaf component, or a `Row` container laying
/// its children out horizontally. Rows exist at compile time only — the emitted package is flat.
#[derive(Clone, Debug, Eq, PartialEq)]
pub enum ScreenItem {
    Component(NodeDefinition),
    Row(RowDefinition),
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct RowDefinition {
    pub id: String,
    pub height: Dimension,
    pub spacing: u16,
    /// Optional background color token: emits a synthetic Panel node spanning the row.
    pub background: Option<String>,
    pub children: Vec<NodeDefinition>,
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct NodeDefinition {
    pub id: String,
    pub width: Dimension,
    pub height: Dimension,
    /// ADR-014 absolute placement: screen coordinates of the top-left corner, out of flow.
    pub position: Option<(u32, u32)>,
    pub kind: NodeKind,
    pub safety_critical: Option<SafetyCriticalDefinition>,
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub enum NodeKind {
    CriticalButton {
        requirement_id: String,
        label_text_key: String,
        color_token: String,
        on_press: SystemEvent,
    },
    VulkanViewport {
        stream_source: String,
    },
    /// A scrolling 2D amplitude trace (ADR-018), e.g. an EEG/ECG waveform — distinct from
    /// `VulkanViewport`'s 3D spectral heightfield.
    SignalTrace {
        stream_source: String,
        color_token: String,
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
    Image {
        image_id: String,
    },
    /// Application-semantic interactive button (ADR-015): no SystemEvent, events by source key.
    Button {
        label_text_key: String,
        color_token: String,
        source: String,
        requirement_id: Option<String>,
    },
    /// Operator-editable text field (ADR-015): bounded controlled component over a baked charset.
    TextInput {
        source: String,
        max_length: u16,
        glyph_set_id: String,
        color_token: String,
        requirement_id: Option<String>,
    },
}

/// An absolute, resolved rectangle in surface coordinates — the authoring-side counterpart of a
/// device-rendered bounding box, produced once layout/positioning has been fully resolved.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct RectSpec {
    pub x: i32,
    pub y: i32,
    pub width: u32,
    pub height: u32,
}

/// A leaf node with its layout fully resolved to absolute bounds — the authoring-side mirror of
/// one entry in `trustsc_ui::CompiledScreenPackage`'s node table, before Rust code generation.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct CompiledNodeSpec {
    pub id: String,
    pub bounds: RectSpec,
    pub kind: NodeKind,
}

/// The authoring-side mirror of a `trustsc_ui::CompiledScreenPackage` golden reference entry:
/// what a rendered frame is checked against for one node (ADR-011, ADR-016).
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct GoldenReferenceSpec {
    pub node_id: String,
    pub bounds: RectSpec,
    pub text_key: Option<String>,
    pub color_token: Option<String>,
    pub cv_checks: Vec<CvCheckKind>,
}

/// A fully compiled screen as owned data — the authoring-side mirror of
/// `trustsc_ui::CompiledScreenPackage`: every node's resolved absolute bounds and every golden
/// reference, without generating or parsing Rust source text. Produced by
/// [`crate::compile_medui_source`] / [`crate::compile_screen_definition`].
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct CompiledScreenSpec {
    pub id: String,
    pub layout: LayoutDefinition,
    pub surface: (u32, u32),
    pub nodes: Vec<CompiledNodeSpec>,
    pub golden_references: Vec<GoldenReferenceSpec>,
}
