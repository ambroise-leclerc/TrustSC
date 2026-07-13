//! JSON DTOs for the studio REST API (ADR-022 wave S8). Mirrors of the governed-crate authoring
//! types, kept entirely in the tool: `crates/trustsc-ui-dsl-authoring` has no serde dependency
//! (S1's ADR), so every `Serialize`/`Deserialize` derive and every conversion between a DTO and
//! its governed-crate counterpart lives here, never there.

use serde::{Deserialize, Serialize};
use trustsc_ui_dsl_authoring::{
    CompiledNodeSpec, CompiledScreenSpec, Diagnostic, Dimension, GoldenReferenceSpec, ImageInfo,
    LayoutDefinition, LocaleEntry, NodeDefinition, NodeKind, NumericTemplateInfo, PropDomain,
    PropSchema, RowDefinition, SafetyCriticalDefinition, ScreenDefinition, ScreenItem,
    Severity, TextKeyInfo, WidgetSchema,
};
use trustsc::{ClockFormat, CvCheckKind, LayoutKind, SystemEvent};

// ---------------------------------------------------------------------------------------------
// Small closed-set enums: trustsc_ui's own types can't derive serde (no serde in crates/), so
// every one gets a local mirror plus a pair of infallible conversions.
// ---------------------------------------------------------------------------------------------

#[derive(Clone, Copy, Debug, Serialize, Deserialize)]
pub enum LayoutKindDto {
    Vertical,
    Horizontal,
}

impl From<LayoutKind> for LayoutKindDto {
    fn from(value: LayoutKind) -> Self {
        match value {
            LayoutKind::Vertical => LayoutKindDto::Vertical,
            LayoutKind::Horizontal => LayoutKindDto::Horizontal,
        }
    }
}

impl From<LayoutKindDto> for LayoutKind {
    fn from(value: LayoutKindDto) -> Self {
        match value {
            LayoutKindDto::Vertical => LayoutKind::Vertical,
            LayoutKindDto::Horizontal => LayoutKind::Horizontal,
        }
    }
}

#[derive(Clone, Copy, Debug, Serialize, Deserialize)]
pub enum SystemEventDto {
    NoOp,
    TriggerHalt,
}

impl From<SystemEvent> for SystemEventDto {
    fn from(value: SystemEvent) -> Self {
        match value {
            SystemEvent::NoOp => SystemEventDto::NoOp,
            SystemEvent::TriggerHalt => SystemEventDto::TriggerHalt,
        }
    }
}

impl From<SystemEventDto> for SystemEvent {
    fn from(value: SystemEventDto) -> Self {
        match value {
            SystemEventDto::NoOp => SystemEvent::NoOp,
            SystemEventDto::TriggerHalt => SystemEvent::TriggerHalt,
        }
    }
}

#[derive(Clone, Copy, Debug, Serialize, Deserialize)]
pub enum ClockFormatDto {
    TimeSeconds,
    DateTimeSeconds,
}

impl From<ClockFormat> for ClockFormatDto {
    fn from(value: ClockFormat) -> Self {
        match value {
            ClockFormat::TimeSeconds => ClockFormatDto::TimeSeconds,
            ClockFormat::DateTimeSeconds => ClockFormatDto::DateTimeSeconds,
        }
    }
}

impl From<ClockFormatDto> for ClockFormat {
    fn from(value: ClockFormatDto) -> Self {
        match value {
            ClockFormatDto::TimeSeconds => ClockFormat::TimeSeconds,
            ClockFormatDto::DateTimeSeconds => ClockFormat::DateTimeSeconds,
        }
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub enum CvCheckKindDto {
    Bounds,
    ColorHash,
}

impl From<CvCheckKind> for CvCheckKindDto {
    fn from(value: CvCheckKind) -> Self {
        match value {
            CvCheckKind::Bounds => CvCheckKindDto::Bounds,
            CvCheckKind::ColorHash => CvCheckKindDto::ColorHash,
        }
    }
}

impl From<CvCheckKindDto> for CvCheckKind {
    fn from(value: CvCheckKindDto) -> Self {
        match value {
            CvCheckKindDto::Bounds => CvCheckKind::Bounds,
            CvCheckKindDto::ColorHash => CvCheckKind::ColorHash,
        }
    }
}

// ---------------------------------------------------------------------------------------------
// AST DTOs (ScreenDefinition and everything it owns) — the shape `screen:` fields carry in both
// directions: served by GET /api/screens/{id}, accepted by POST /api/compile, /api/frame, and
// /api/serialize.
// ---------------------------------------------------------------------------------------------

#[derive(Clone, Copy, Debug, Serialize, Deserialize)]
#[serde(tag = "kind")]
pub enum DimensionDto {
    Px { value: u32 },
    Fill,
}

impl From<Dimension> for DimensionDto {
    fn from(value: Dimension) -> Self {
        match value {
            Dimension::Px(value) => DimensionDto::Px { value },
            Dimension::Fill => DimensionDto::Fill,
        }
    }
}

impl From<DimensionDto> for Dimension {
    fn from(value: DimensionDto) -> Self {
        match value {
            DimensionDto::Px { value } => Dimension::Px(value),
            DimensionDto::Fill => Dimension::Fill,
        }
    }
}

#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct LayoutDefinitionDto {
    pub kind: LayoutKindDto,
    pub spacing: u16,
    pub padding: u16,
}

impl From<LayoutDefinition> for LayoutDefinitionDto {
    fn from(value: LayoutDefinition) -> Self {
        LayoutDefinitionDto {
            kind: value.kind.into(),
            spacing: value.spacing,
            padding: value.padding,
        }
    }
}

impl From<LayoutDefinitionDto> for LayoutDefinition {
    fn from(value: LayoutDefinitionDto) -> Self {
        LayoutDefinition {
            kind: value.kind.into(),
            spacing: value.spacing,
            padding: value.padding,
        }
    }
}

#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct SafetyCriticalDto {
    pub cv_checks: Vec<CvCheckKindDto>,
}

impl From<SafetyCriticalDefinition> for SafetyCriticalDto {
    fn from(value: SafetyCriticalDefinition) -> Self {
        SafetyCriticalDto {
            cv_checks: value.cv_checks.into_iter().map(CvCheckKindDto::from).collect(),
        }
    }
}

impl From<SafetyCriticalDto> for SafetyCriticalDefinition {
    fn from(value: SafetyCriticalDto) -> Self {
        SafetyCriticalDefinition {
            cv_checks: value.cv_checks.into_iter().map(CvCheckKind::from).collect(),
        }
    }
}

/// Mirrors [`NodeKind`] exactly, including `Panel` — which a compiled node summary can
/// legitimately carry (synthesized from a `Row`'s `background:`) even though an authored/edited
/// AST never should. Endpoints that accept an AST DTO reject a submitted `Panel` explicitly
/// rather than passing it through (see `api::reject_panel_nodes`).
#[derive(Clone, Debug, Serialize, Deserialize)]
#[serde(tag = "kind")]
pub enum NodeKindDto {
    CriticalButton {
        requirement_id: String,
        label_text_key: String,
        color_token: String,
        on_press: SystemEventDto,
    },
    VulkanViewport {
        stream_source: String,
    },
    SignalTrace {
        stream_source: String,
        color_token: String,
    },
    Label {
        text_key: String,
        color_token: String,
    },
    Clock {
        format: ClockFormatDto,
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
    Panel {
        color_token: String,
    },
    Image {
        image_id: String,
    },
    Button {
        label_text_key: String,
        color_token: String,
        source: String,
        requirement_id: Option<String>,
    },
    TextInput {
        source: String,
        max_length: u16,
        glyph_set_id: String,
        color_token: String,
        requirement_id: Option<String>,
    },
}

impl From<NodeKind> for NodeKindDto {
    fn from(value: NodeKind) -> Self {
        match value {
            NodeKind::CriticalButton {
                requirement_id,
                label_text_key,
                color_token,
                on_press,
            } => NodeKindDto::CriticalButton {
                requirement_id,
                label_text_key,
                color_token,
                on_press: on_press.into(),
            },
            NodeKind::VulkanViewport { stream_source } => {
                NodeKindDto::VulkanViewport { stream_source }
            }
            NodeKind::SignalTrace { stream_source, color_token } => {
                NodeKindDto::SignalTrace { stream_source, color_token }
            }
            NodeKind::Label { text_key, color_token } => {
                NodeKindDto::Label { text_key, color_token }
            }
            NodeKind::Clock { format } => NodeKindDto::Clock { format: format.into() },
            NodeKind::NumericDisplay {
                requirement_id,
                template_id,
                source,
                color_token,
            } => NodeKindDto::NumericDisplay {
                requirement_id,
                template_id,
                source,
                color_token,
            },
            NodeKind::StatusIndicator {
                requirement_id,
                source,
                state_text_keys,
                color_tokens,
            } => NodeKindDto::StatusIndicator {
                requirement_id,
                source,
                state_text_keys,
                color_tokens,
            },
            NodeKind::Panel { color_token } => NodeKindDto::Panel { color_token },
            NodeKind::Image { image_id } => NodeKindDto::Image { image_id },
            NodeKind::Button {
                label_text_key,
                color_token,
                source,
                requirement_id,
            } => NodeKindDto::Button {
                label_text_key,
                color_token,
                source,
                requirement_id,
            },
            NodeKind::TextInput {
                source,
                max_length,
                glyph_set_id,
                color_token,
                requirement_id,
            } => NodeKindDto::TextInput {
                source,
                max_length,
                glyph_set_id,
                color_token,
                requirement_id,
            },
        }
    }
}

impl From<NodeKindDto> for NodeKind {
    fn from(value: NodeKindDto) -> Self {
        match value {
            NodeKindDto::CriticalButton {
                requirement_id,
                label_text_key,
                color_token,
                on_press,
            } => NodeKind::CriticalButton {
                requirement_id,
                label_text_key,
                color_token,
                on_press: on_press.into(),
            },
            NodeKindDto::VulkanViewport { stream_source } => {
                NodeKind::VulkanViewport { stream_source }
            }
            NodeKindDto::SignalTrace { stream_source, color_token } => {
                NodeKind::SignalTrace { stream_source, color_token }
            }
            NodeKindDto::Label { text_key, color_token } => {
                NodeKind::Label { text_key, color_token }
            }
            NodeKindDto::Clock { format } => NodeKind::Clock { format: format.into() },
            NodeKindDto::NumericDisplay {
                requirement_id,
                template_id,
                source,
                color_token,
            } => NodeKind::NumericDisplay {
                requirement_id,
                template_id,
                source,
                color_token,
            },
            NodeKindDto::StatusIndicator {
                requirement_id,
                source,
                state_text_keys,
                color_tokens,
            } => NodeKind::StatusIndicator {
                requirement_id,
                source,
                state_text_keys,
                color_tokens,
            },
            NodeKindDto::Panel { color_token } => NodeKind::Panel { color_token },
            NodeKindDto::Image { image_id } => NodeKind::Image { image_id },
            NodeKindDto::Button {
                label_text_key,
                color_token,
                source,
                requirement_id,
            } => NodeKind::Button {
                label_text_key,
                color_token,
                source,
                requirement_id,
            },
            NodeKindDto::TextInput {
                source,
                max_length,
                glyph_set_id,
                color_token,
                requirement_id,
            } => NodeKind::TextInput {
                source,
                max_length,
                glyph_set_id,
                color_token,
                requirement_id,
            },
        }
    }
}

/// True for the one [`NodeKindDto`] variant an authored/edited AST must never contain — `Panel`
/// is synthesized by the compiler from a `Row`'s `background:` and has no `.medui` syntax of its
/// own (S4's serializer panics on it; API handlers reject it before ever reaching that code).
pub fn is_panel(kind: &NodeKindDto) -> bool {
    matches!(kind, NodeKindDto::Panel { .. })
}

#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct NodeDefinitionDto {
    pub id: String,
    pub width: DimensionDto,
    pub height: DimensionDto,
    pub position: Option<(u32, u32)>,
    pub kind: NodeKindDto,
    pub safety_critical: Option<SafetyCriticalDto>,
}

impl From<NodeDefinition> for NodeDefinitionDto {
    fn from(value: NodeDefinition) -> Self {
        NodeDefinitionDto {
            id: value.id,
            width: value.width.into(),
            height: value.height.into(),
            position: value.position,
            kind: value.kind.into(),
            safety_critical: value.safety_critical.map(SafetyCriticalDto::from),
        }
    }
}

impl From<NodeDefinitionDto> for NodeDefinition {
    fn from(value: NodeDefinitionDto) -> Self {
        NodeDefinition {
            id: value.id,
            width: value.width.into(),
            height: value.height.into(),
            position: value.position,
            kind: value.kind.into(),
            safety_critical: value.safety_critical.map(SafetyCriticalDefinition::from),
        }
    }
}

#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct RowDefinitionDto {
    pub id: String,
    pub height: DimensionDto,
    pub spacing: u16,
    pub background: Option<String>,
    pub children: Vec<NodeDefinitionDto>,
}

impl From<RowDefinition> for RowDefinitionDto {
    fn from(value: RowDefinition) -> Self {
        RowDefinitionDto {
            id: value.id,
            height: value.height.into(),
            spacing: value.spacing,
            background: value.background,
            children: value.children.into_iter().map(NodeDefinitionDto::from).collect(),
        }
    }
}

impl From<RowDefinitionDto> for RowDefinition {
    fn from(value: RowDefinitionDto) -> Self {
        RowDefinition {
            id: value.id,
            height: value.height.into(),
            spacing: value.spacing,
            background: value.background,
            children: value.children.into_iter().map(NodeDefinition::from).collect(),
        }
    }
}

// `type`, not `kind`: `Component`'s payload (`NodeDefinitionDto`) already has its own `kind`
// field (the widget kind), and an internally tagged enum merges its tag into the payload's own
// map — reusing "kind" here would collide with that field and corrupt both on serialization.
#[derive(Clone, Debug, Serialize, Deserialize)]
#[serde(tag = "type")]
pub enum ScreenItemDto {
    Component(NodeDefinitionDto),
    Row(RowDefinitionDto),
}

impl From<ScreenItem> for ScreenItemDto {
    fn from(value: ScreenItem) -> Self {
        match value {
            ScreenItem::Component(node) => ScreenItemDto::Component(node.into()),
            ScreenItem::Row(row) => ScreenItemDto::Row(row.into()),
        }
    }
}

impl From<ScreenItemDto> for ScreenItem {
    fn from(value: ScreenItemDto) -> Self {
        match value {
            ScreenItemDto::Component(node) => ScreenItem::Component(node.into()),
            ScreenItemDto::Row(row) => ScreenItem::Row(row.into()),
        }
    }
}

#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct ScreenDefinitionDto {
    pub id: String,
    pub layout: LayoutDefinitionDto,
    pub declared_surface: Option<(u32, u32)>,
    pub items: Vec<ScreenItemDto>,
}

impl From<ScreenDefinition> for ScreenDefinitionDto {
    fn from(value: ScreenDefinition) -> Self {
        ScreenDefinitionDto {
            id: value.id,
            layout: value.layout.into(),
            declared_surface: value.declared_surface,
            items: value.items.into_iter().map(ScreenItemDto::from).collect(),
        }
    }
}

impl From<ScreenDefinitionDto> for ScreenDefinition {
    fn from(value: ScreenDefinitionDto) -> Self {
        ScreenDefinition {
            id: value.id,
            layout: value.layout.into(),
            declared_surface: value.declared_surface,
            items: value.items.into_iter().map(ScreenItem::from).collect(),
        }
    }
}

/// Every [`NodeKindDto::Panel`] reachable from a screen DTO's top-level items or row children —
/// used to reject a submitted AST containing one before it ever reaches `compile_screen_definition`
/// or `serialize_screen`.
pub fn contains_panel(screen: &ScreenDefinitionDto) -> bool {
    screen.items.iter().any(|item| match item {
        ScreenItemDto::Component(node) => is_panel(&node.kind),
        ScreenItemDto::Row(row) => row.children.iter().any(|child| is_panel(&child.kind)),
    })
}

// ---------------------------------------------------------------------------------------------
// Diagnostics
// ---------------------------------------------------------------------------------------------

#[derive(Clone, Copy, Debug, Serialize)]
pub enum SeverityDto {
    Error,
}

impl From<Severity> for SeverityDto {
    fn from(value: Severity) -> Self {
        match value {
            Severity::Error => SeverityDto::Error,
        }
    }
}

#[derive(Clone, Debug, Serialize)]
pub struct DiagnosticDto {
    pub message: String,
    pub line: Option<u32>,
    pub severity: SeverityDto,
}

impl From<Diagnostic> for DiagnosticDto {
    fn from(value: Diagnostic) -> Self {
        DiagnosticDto {
            message: value.message,
            line: value.line,
            severity: value.severity.into(),
        }
    }
}

pub fn diagnostics_to_dto(diagnostics: Vec<Diagnostic>) -> Vec<DiagnosticDto> {
    diagnostics.into_iter().map(DiagnosticDto::from).collect()
}

// ---------------------------------------------------------------------------------------------
// Compiled-node summaries — `GET /api/screens/{id}` and `POST /api/compile`'s `compiled.nodes`.
// Deliberately a flattened summary, not a full `CompiledNodeSpec` mirror: `kind` names the node
// and carries its fields (same `NodeKindDto` the AST uses — `Panel` included, since a compiled
// node list can legitimately contain compiler-synthesized Panels), `safety_critical` is whether
// the *authored* node carried a `@safety_critical` annotation... which the flat `CompiledNodeSpec`
// doesn't track (that lives on the pre-compile `NodeDefinition`only) — so this is derived from
// whether a golden reference exists for the node id instead, the same signal the compiler itself
// uses to decide "this node has evidence".
#[derive(Clone, Debug, Serialize)]
pub struct BoundsDto {
    pub x: i32,
    pub y: i32,
    pub w: u32,
    pub h: u32,
}

#[derive(Clone, Debug, Serialize)]
pub struct CompiledNodeSummaryDto {
    pub id: String,
    pub kind: NodeKindDto,
    pub bounds: BoundsDto,
    pub safety_critical: bool,
    pub golden_checks: Vec<CvCheckKindDto>,
}

fn compiled_node_to_dto(node: CompiledNodeSpec, golden: &[GoldenReferenceSpec]) -> CompiledNodeSummaryDto {
    let golden_checks = golden
        .iter()
        .find(|entry| entry.node_id == node.id)
        .map(|entry| entry.cv_checks.iter().copied().map(CvCheckKindDto::from).collect())
        .unwrap_or_default();
    CompiledNodeSummaryDto {
        id: node.id.clone(),
        safety_critical: golden.iter().any(|entry| entry.node_id == node.id),
        kind: node.kind.into(),
        bounds: BoundsDto {
            x: node.bounds.x,
            y: node.bounds.y,
            w: node.bounds.width,
            h: node.bounds.height,
        },
        golden_checks,
    }
}

#[derive(Clone, Debug, Serialize)]
pub struct CompiledSummaryDto {
    pub surface: (u32, u32),
    pub nodes: Vec<CompiledNodeSummaryDto>,
}

pub fn compiled_summary_from_spec(spec: CompiledScreenSpec) -> CompiledSummaryDto {
    let surface = spec.surface;
    let golden = spec.golden_references;
    let nodes = spec
        .nodes
        .into_iter()
        .map(|node| compiled_node_to_dto(node, &golden))
        .collect();
    CompiledSummaryDto { surface, nodes }
}

// ---------------------------------------------------------------------------------------------
// Palette (`GET /api/palette`)
// ---------------------------------------------------------------------------------------------

#[derive(Clone, Debug, Serialize)]
#[serde(tag = "kind")]
pub enum PropDomainDto {
    Identifier,
    DimensionPx { fill_allowed: bool },
    Position,
    TextKey,
    TextKeyList,
    ColorToken,
    ColorTokenList,
    QuotedSource,
    StreamSource,
    TemplateId,
    ImageRef,
    SystemEvent,
    ClockFormat,
    Charset,
    MaxLength,
    RequirementId { optional: bool },
}

impl From<PropDomain> for PropDomainDto {
    fn from(value: PropDomain) -> Self {
        match value {
            PropDomain::Identifier => PropDomainDto::Identifier,
            PropDomain::DimensionPx { fill_allowed } => PropDomainDto::DimensionPx { fill_allowed },
            PropDomain::Position => PropDomainDto::Position,
            PropDomain::TextKey => PropDomainDto::TextKey,
            PropDomain::TextKeyList => PropDomainDto::TextKeyList,
            PropDomain::ColorToken => PropDomainDto::ColorToken,
            PropDomain::ColorTokenList => PropDomainDto::ColorTokenList,
            PropDomain::QuotedSource => PropDomainDto::QuotedSource,
            PropDomain::StreamSource => PropDomainDto::StreamSource,
            PropDomain::TemplateId => PropDomainDto::TemplateId,
            PropDomain::ImageRef => PropDomainDto::ImageRef,
            PropDomain::SystemEvent => PropDomainDto::SystemEvent,
            PropDomain::ClockFormat => PropDomainDto::ClockFormat,
            PropDomain::Charset => PropDomainDto::Charset,
            PropDomain::MaxLength => PropDomainDto::MaxLength,
            PropDomain::RequirementId { optional } => PropDomainDto::RequirementId { optional },
        }
    }
}

#[derive(Clone, Debug, Serialize)]
pub struct PropSchemaDto {
    pub key: String,
    pub required: bool,
    pub domain: PropDomainDto,
}

impl From<&PropSchema> for PropSchemaDto {
    fn from(value: &PropSchema) -> Self {
        PropSchemaDto {
            key: value.key.to_string(),
            required: value.required,
            domain: value.domain.into(),
        }
    }
}

#[derive(Clone, Debug, Serialize)]
pub struct WidgetSchemaDto {
    pub kind_name: String,
    pub description: String,
    pub safety_critical_eligible: bool,
    pub properties: Vec<PropSchemaDto>,
}

impl From<&WidgetSchema> for WidgetSchemaDto {
    fn from(value: &WidgetSchema) -> Self {
        WidgetSchemaDto {
            kind_name: value.kind_name.to_string(),
            description: value.description.to_string(),
            safety_critical_eligible: value.safety_critical_eligible,
            properties: value.properties.iter().map(PropSchemaDto::from).collect(),
        }
    }
}

#[derive(Clone, Debug, Serialize)]
pub struct ColorSwatchDto {
    pub token: String,
    pub rgba: [f32; 4],
}

#[derive(Clone, Debug, Serialize)]
pub struct LocaleEntryDto {
    pub locale: String,
    pub value: String,
    pub width_px: u32,
    pub height_px: u32,
}

impl From<LocaleEntry> for LocaleEntryDto {
    fn from(value: LocaleEntry) -> Self {
        LocaleEntryDto {
            locale: value.locale,
            value: value.value,
            width_px: value.width_px,
            height_px: value.height_px,
        }
    }
}

#[derive(Clone, Debug, Serialize)]
pub struct TextKeyInfoDto {
    pub string_id: String,
    pub entries: Vec<LocaleEntryDto>,
}

impl From<TextKeyInfo> for TextKeyInfoDto {
    fn from(value: TextKeyInfo) -> Self {
        TextKeyInfoDto {
            string_id: value.string_id,
            entries: value.entries.into_iter().map(LocaleEntryDto::from).collect(),
        }
    }
}

#[derive(Clone, Debug, Serialize)]
pub struct NumericTemplateInfoDto {
    pub id: String,
    pub locale: String,
    pub max_chars: u8,
    pub glyph_set_id: String,
}

impl From<NumericTemplateInfo> for NumericTemplateInfoDto {
    fn from(value: NumericTemplateInfo) -> Self {
        NumericTemplateInfoDto {
            id: value.id,
            locale: value.locale,
            max_chars: value.max_chars,
            glyph_set_id: value.glyph_set_id,
        }
    }
}

#[derive(Clone, Debug, Serialize)]
pub struct ImageInfoDto {
    pub id: String,
    pub width: u32,
    pub height: u32,
}

impl From<ImageInfo> for ImageInfoDto {
    fn from(value: ImageInfo) -> Self {
        ImageInfoDto {
            id: value.id,
            width: value.width,
            height: value.height,
        }
    }
}

#[derive(Clone, Debug, Serialize)]
pub struct PaletteDto {
    pub widgets: Vec<WidgetSchemaDto>,
    pub colors: Vec<ColorSwatchDto>,
    pub text_keys: Vec<TextKeyInfoDto>,
    pub templates: Vec<NumericTemplateInfoDto>,
    pub images: Vec<ImageInfoDto>,
    pub locales: Vec<String>,
}
