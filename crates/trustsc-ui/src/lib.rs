#![forbid(unsafe_code)]

use trustsc_core::{DeterminismPolicy, MduxResult, Validates, ValidationError};
use trustsc_governance::RequirementId;

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum LayoutKind {
    Vertical,
    Horizontal,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum GraphicsProfile {
    Vulkan,
    VulkanSc,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum PipelineMode {
    Dynamic,
    OfflineCompiled,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum CvCheckKind {
    Bounds,
    ColorHash,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum SystemEvent {
    NoOp,
    TriggerHalt,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct Rect {
    pub x: i32,
    pub y: i32,
    pub width: u32,
    pub height: u32,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct LayoutSpec {
    pub kind: LayoutKind,
    pub spacing: u16,
    pub padding: u16,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct CriticalButtonSpec {
    pub requirement_id: &'static str,
    pub text_key: &'static str,
    pub color_token: &'static str,
    pub on_press: SystemEvent,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct ViewportReservation {
    pub stream_source: &'static str,
}

/// Reserved region for a scrolling 2D amplitude trace (ADR-018) — a single-channel signal such
/// as an EEG/ECG waveform, distinct from `ViewportReservation`'s 3D spectral heightfield. Compiles
/// into a region descriptor only; the live samples arrive each frame through the realtime path
/// (`trustsc::realtime::FrameInputs::push_sample`), never through the UI package itself.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct SignalTraceSpec {
    pub stream_source: &'static str,
    pub color_token: &'static str,
}

/// Static approved text with no interaction and no requirement of its own (titles, units).
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct LabelSpec {
    pub text_key: &'static str,
    pub color_token: &'static str,
}

/// Wall-clock format rendered by the platform adapter from the system clock.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum ClockFormat {
    /// `HH:MM:SS`
    TimeSeconds,
    /// `YYYY-MM-DD HH:MM:SS`
    DateTimeSeconds,
}

/// Realtime clock display. Content comes from the platform clock via the adapter — the
/// application writes no code for it — so it carries neither an approved text key nor a
/// requirement.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct ClockSpec {
    pub format: ClockFormat,
}

/// Realtime numeric value bound to an approved `NumericTemplate` and a named data source fed
/// by the application each frame. Requirement-bearing and eligible for `@safety_critical`.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct NumericDisplaySpec {
    pub requirement_id: &'static str,
    pub template_id: &'static str,
    pub source: &'static str,
    pub color_token: &'static str,
}

/// Enumerated device-state display: `state_text_keys[i]` is the approved label shown when the
/// application selects state `i`, tinted with `color_tokens[i]`. Requirement-bearing and
/// eligible for `@safety_critical`.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct StatusIndicatorSpec {
    pub requirement_id: &'static str,
    pub source: &'static str,
    pub state_text_keys: &'static [&'static str],
    pub color_tokens: &'static [&'static str],
}

impl StatusIndicatorSpec {
    /// Checks the invariant every consumer relies on before indexing `state_text_keys` and
    /// `color_tokens` in lockstep: same non-zero length. The MedUI DSL compiler already
    /// guarantees this for generated screens, but the fields are public, so anything built by
    /// hand (or by a future authoring path) must be checked before use rather than trusted.
    pub fn validate(&self) -> MduxResult<()> {
        if self.state_text_keys.is_empty() {
            return Err(ValidationError::new(
                "status indicator must declare at least one state",
            ));
        }
        if self.state_text_keys.len() != self.color_tokens.len() {
            return Err(ValidationError::new(
                "status indicator state_text_keys and color_tokens must have the same length",
            ));
        }
        Ok(())
    }
}

/// Solid-color background rectangle synthesized by the MedUI compiler (e.g. from a Row's
/// `background:` property). Underlay by definition: no requirement, no text, exempt from the
/// ADR-014 overlap rule, drawn beneath every other node.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct PanelSpec {
    pub color_token: &'static str,
}

/// Governed raster image (ADR-014): rendered at its intrinsic size only — the compiler verifies
/// the declared bounds equal the baked package's dimensions exactly, so there is no runtime
/// scaling.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct ImageSpec {
    pub image_id: &'static str,
}

/// Application-semantic interactive button (ADR-015). Its static approved label rides the
/// startup text path like a `Label`; a press is delivered to the application as a
/// `ButtonPressed { source }` event through the bounded outbound event plane — by data, not by
/// callback. Deliberately carries no `SystemEvent`: framework-governed actions belong to
/// `CriticalButton`.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct ButtonSpec {
    pub text_key: &'static str,
    pub color_token: &'static str,
    pub source: &'static str,
    pub requirement_id: Option<&'static str>,
}

impl ButtonSpec {
    /// Checks the non-emptiness invariants the MedUI compiler guarantees for generated screens.
    /// The fields are public, so anything built by hand must be checked before use rather than
    /// trusted.
    pub fn validate(&self) -> MduxResult<()> {
        if self.text_key.trim().is_empty() {
            return Err(ValidationError::new("button text_key must not be empty"));
        }
        if self.color_token.trim().is_empty() {
            return Err(ValidationError::new("button color_token must not be empty"));
        }
        if self.source.trim().is_empty() {
            return Err(ValidationError::new("button source must not be empty"));
        }
        if matches!(self.requirement_id, Some(requirement_id) if requirement_id.trim().is_empty())
        {
            return Err(ValidationError::new(
                "button requirement_id must not be empty when declared",
            ));
        }
        Ok(())
    }
}

/// Operator-editable text field (ADR-015): a controlled component. The application owns the
/// buffer and echoes its content each frame through the bounded realtime path, so the renderer
/// stores nothing; content is restricted to the baked glyph set `glyph_set_id` and to
/// `max_length` characters, both enforced at the frame-input boundary and — for the width
/// budget — at compile time. Like `NumericDisplay`, golden references pin the box and tint,
/// never the varying content.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct TextInputSpec {
    pub source: &'static str,
    pub max_length: u16,
    pub glyph_set_id: &'static str,
    pub color_token: &'static str,
    pub requirement_id: Option<&'static str>,
}

impl TextInputSpec {
    /// Checks the invariants every consumer relies on: a non-zero capacity and non-empty
    /// source/glyph-set/color tokens. The MedUI DSL compiler already guarantees this for
    /// generated screens, but the fields are public, so anything built by hand must be checked
    /// before use rather than trusted.
    pub fn validate(&self) -> MduxResult<()> {
        if self.max_length == 0 {
            return Err(ValidationError::new(
                "text input max_length must be greater than zero",
            ));
        }
        if self.source.trim().is_empty() {
            return Err(ValidationError::new("text input source must not be empty"));
        }
        if self.glyph_set_id.trim().is_empty() {
            return Err(ValidationError::new(
                "text input glyph_set_id must not be empty",
            ));
        }
        if self.color_token.trim().is_empty() {
            return Err(ValidationError::new(
                "text input color_token must not be empty",
            ));
        }
        if matches!(self.requirement_id, Some(requirement_id) if requirement_id.trim().is_empty())
        {
            return Err(ValidationError::new(
                "text input requirement_id must not be empty when declared",
            ));
        }
        Ok(())
    }
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum CompiledNodeKind {
    CriticalButton(CriticalButtonSpec),
    VulkanViewport(ViewportReservation),
    SignalTrace(SignalTraceSpec),
    Label(LabelSpec),
    Clock(ClockSpec),
    NumericDisplay(NumericDisplaySpec),
    StatusIndicator(StatusIndicatorSpec),
    Panel(PanelSpec),
    Image(ImageSpec),
    Button(ButtonSpec),
    TextInput(TextInputSpec),
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct CompiledNode {
    pub id: &'static str,
    pub bounds: Rect,
    pub kind: CompiledNodeKind,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct GoldenReferenceEntry {
    pub node_id: &'static str,
    pub bounds: Rect,
    pub text_key: Option<&'static str>,
    pub color_token: Option<&'static str>,
    pub cv_checks: &'static [CvCheckKind],
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct CompiledScreenPackage {
    pub screen_id: &'static str,
    pub layout: LayoutSpec,
    pub nodes: &'static [CompiledNode],
    pub golden_references: &'static [GoldenReferenceEntry],
}

impl CompiledNodeKind {
    /// The requirement implemented by this node, when it is a traced interactive/critical
    /// element. `Label` and `Clock` are deliberately untraced (decorative / platform-fed);
    /// `Button` and `TextInput` are traced only when they declare a requirement.
    pub fn requirement_id(&self) -> Option<&'static str> {
        match self {
            Self::CriticalButton(specification) => Some(specification.requirement_id),
            Self::NumericDisplay(specification) => Some(specification.requirement_id),
            Self::StatusIndicator(specification) => Some(specification.requirement_id),
            Self::Button(specification) => specification.requirement_id,
            Self::TextInput(specification) => specification.requirement_id,
            Self::VulkanViewport(_)
            | Self::SignalTrace(_)
            | Self::Label(_)
            | Self::Clock(_)
            | Self::Panel(_)
            | Self::Image(_) => None,
        }
    }

    /// The approved string rendered *statically* by this node. Dynamic kinds (`Clock`,
    /// `NumericDisplay`, `StatusIndicator`) return `None`: their glyphs come from the realtime
    /// path each frame, not from the startup `ScreenTextLayout`. Consequently `build()`'s
    /// existing dual-`Some` derivation skips them — deriving their `UiComponent`s is the
    /// realtime bindings layer's job.
    pub fn text_key(&self) -> Option<&'static str> {
        match self {
            Self::CriticalButton(specification) => Some(specification.text_key),
            Self::Label(specification) => Some(specification.text_key),
            Self::Button(specification) => Some(specification.text_key),
            Self::VulkanViewport(_)
            | Self::SignalTrace(_)
            | Self::Clock(_)
            | Self::NumericDisplay(_)
            | Self::StatusIndicator(_)
            | Self::Panel(_)
            | Self::Image(_)
            | Self::TextInput(_) => None,
        }
    }
}

/// The single approved token → RGBA table (ADR-014): the governed source of truth every color
/// token must resolve against. Per the ADR-014 rollout, the MedUI compiler validates every
/// color-bearing property against this table (an unknown token becomes a compile error) and the
/// adapter resolves Panel colors through it at binding time. Linear RGBA, straight alpha.
pub const THEME_COLORS: &[(&str, [f32; 4])] = &[
    ("Theme.Colors.TopbarBackground", [0.82, 0.84, 0.86, 1.0]),
    ("Theme.Colors.Title", [0.10, 0.12, 0.16, 1.0]),
    ("Theme.Colors.ScoreDigits", [0.13, 0.72, 0.42, 1.0]),
    ("Theme.Colors.Nominal", [0.13, 0.72, 0.42, 1.0]),
    ("Theme.Colors.Alert", [0.95, 0.65, 0.15, 1.0]),
    ("Theme.Colors.Fault", [0.86, 0.20, 0.18, 1.0]),
    ("Theme.Colors.Neutral", [0.62, 0.66, 0.70, 1.0]),
    ("Theme.Colors.PrimaryAction", [0.16, 0.44, 0.86, 1.0]),
];

/// Looks a theme color token up in [`THEME_COLORS`]; `None` for unknown tokens.
pub fn resolve_color_token(token: &str) -> Option<[f32; 4]> {
    THEME_COLORS
        .iter()
        .find(|(candidate, _)| *candidate == token)
        .map(|(_, rgba)| *rgba)
}

impl CompiledScreenPackage {
    pub fn find_node(&self, node_id: &str) -> Option<&CompiledNode> {
        self.nodes.iter().find(|node| node.id == node_id)
    }

    pub fn first_critical_button(&self) -> Option<&CompiledNode> {
        self.nodes
            .iter()
            .find(|node| matches!(node.kind, CompiledNodeKind::CriticalButton(_)))
    }
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct UiComponent {
    pub id: String,
    pub label: String,
    pub requirement_ids: Vec<RequirementId>,
}

impl UiComponent {
    pub fn new(
        id: impl Into<String>,
        label: impl Into<String>,
        requirement_ids: Vec<RequirementId>,
    ) -> MduxResult<Self> {
        let component = Self {
            id: id.into(),
            label: label.into(),
            requirement_ids,
        };

        component.validate()?;
        Ok(component)
    }
}

impl Validates for UiComponent {
    fn validate(&self) -> MduxResult<()> {
        if self.id.trim().is_empty() {
            return Err(ValidationError::new("ui component id must not be empty"));
        }

        if self.label.trim().is_empty() {
            return Err(ValidationError::new("ui component label must not be empty"));
        }

        if self.requirement_ids.is_empty() {
            return Err(ValidationError::new(
                "ui component must reference at least one requirement",
            ));
        }

        Ok(())
    }
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct UiSdkConfig {
    pub graphics_profile: GraphicsProfile,
    pub width: u32,
    pub height: u32,
    pub pipeline_mode: PipelineMode,
    pub determinism_policy: DeterminismPolicy,
    pub reserved_memory_bytes: u64,
    pub reserved_descriptor_sets: u32,
}

impl UiSdkConfig {
    pub fn vulkan_class_b(width: u32, height: u32, max_frame_time_ms: u32) -> Self {
        Self {
            graphics_profile: GraphicsProfile::Vulkan,
            width,
            height,
            pipeline_mode: PipelineMode::Dynamic,
            determinism_policy: DeterminismPolicy::standard(max_frame_time_ms),
            reserved_memory_bytes: 0,
            reserved_descriptor_sets: 0,
        }
    }

    pub fn vulkansc_class_c(
        width: u32,
        height: u32,
        max_frame_time_ms: u32,
        reserved_memory_bytes: u64,
        reserved_descriptor_sets: u32,
    ) -> Self {
        Self {
            graphics_profile: GraphicsProfile::VulkanSc,
            width,
            height,
            pipeline_mode: PipelineMode::OfflineCompiled,
            determinism_policy: DeterminismPolicy::vulkan_sc(max_frame_time_ms),
            reserved_memory_bytes,
            reserved_descriptor_sets,
        }
    }

    pub fn profile_name(&self) -> &'static str {
        match self.graphics_profile {
            GraphicsProfile::Vulkan => "Vulkan",
            GraphicsProfile::VulkanSc => "Vulkan SC",
        }
    }
}

impl Validates for UiSdkConfig {
    fn validate(&self) -> MduxResult<()> {
        if self.width == 0 || self.height == 0 {
            return Err(ValidationError::new(
                "ui dimensions must be greater than zero",
            ));
        }

        if self.determinism_policy.max_frame_time_ms == 0 {
            return Err(ValidationError::new(
                "max frame time must be greater than zero",
            ));
        }

        if self.graphics_profile == GraphicsProfile::VulkanSc {
            if self.pipeline_mode != PipelineMode::OfflineCompiled {
                return Err(ValidationError::new(
                    "Vulkan SC requires offline compiled pipelines",
                ));
            }

            if self.determinism_policy.runtime_allocation_allowed {
                return Err(ValidationError::new(
                    "Vulkan SC does not allow runtime allocations",
                ));
            }

            if self.determinism_policy.runtime_object_creation_allowed {
                return Err(ValidationError::new(
                    "Vulkan SC does not allow runtime object creation",
                ));
            }

            if !self.determinism_policy.offline_pipeline_required {
                return Err(ValidationError::new(
                    "Vulkan SC requires offline pipeline validation",
                ));
            }

            if self.reserved_memory_bytes == 0 || self.reserved_descriptor_sets == 0 {
                return Err(ValidationError::new(
                    "Vulkan SC requires explicit reserved memory and descriptor budgets",
                ));
            }
        }

        Ok(())
    }
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct FrameStatistics {
    pub frame_index: u64,
    pub draw_calls: u32,
    pub frame_time_ms: u32,
    pub dynamic_allocations: u32,
}

pub struct MedicalUiRuntime {
    config: UiSdkConfig,
    components: Vec<UiComponent>,
}

impl MedicalUiRuntime {
    pub fn new(config: UiSdkConfig, components: Vec<UiComponent>) -> MduxResult<Self> {
        config.validate()?;

        if components.is_empty() {
            return Err(ValidationError::new(
                "ui runtime must contain at least one component",
            ));
        }

        for component in &components {
            component.validate()?;
        }

        Ok(Self { config, components })
    }

    pub fn config(&self) -> &UiSdkConfig {
        &self.config
    }

    pub fn components(&self) -> &[UiComponent] {
        &self.components
    }

    pub fn render_frame(&self, frame_index: u64) -> FrameStatistics {
        let draw_calls = self.components.len() as u32;
        let dynamic_allocations = if self.config.determinism_policy.runtime_allocation_allowed {
            draw_calls.max(1)
        } else {
            0
        };
        let estimated_frame_time_ms =
            (draw_calls.max(1) * 2).min(self.config.determinism_policy.max_frame_time_ms);

        FrameStatistics {
            frame_index,
            draw_calls,
            frame_time_ms: estimated_frame_time_ms,
            dynamic_allocations,
        }
    }

    pub fn compliance_snapshot(&self) -> String {
        format!(
            "profile={} components={} reserved_memory_bytes={} reserved_descriptor_sets={}",
            self.config.profile_name(),
            self.components.len(),
            self.config.reserved_memory_bytes,
            self.config.reserved_descriptor_sets
        )
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn vulkansc_requires_reserved_budgets() {
        let config = UiSdkConfig::vulkansc_class_c(1024, 600, 16, 0, 64);
        let error = config.validate().expect_err("reserved memory is required");

        assert_eq!(
            error.to_string(),
            "Vulkan SC requires explicit reserved memory and descriptor budgets"
        );
    }

    #[test]
    fn theme_color_table_resolves_every_entry_and_rejects_unknown_tokens() {
        for (token, rgba) in THEME_COLORS {
            assert_eq!(resolve_color_token(token), Some(*rgba), "{token}");
        }
        assert_eq!(resolve_color_token("Theme.Colors.DoesNotExist"), None);
        assert_eq!(
            resolve_color_token("Theme.Colors.TopbarBackground"),
            Some([0.82, 0.84, 0.86, 1.0])
        );
    }

    #[test]
    fn panel_and_image_kinds_are_const_constructible_and_untraced() {
        const PANEL: CompiledNode = CompiledNode {
            id: "topbar-background",
            bounds: Rect { x: 0, y: 0, width: 1920, height: 64 },
            kind: CompiledNodeKind::Panel(PanelSpec {
                color_token: "Theme.Colors.TopbarBackground",
            }),
        };
        const IMAGE: CompiledNode = CompiledNode {
            id: "acme-logo",
            bounds: Rect { x: 1768, y: 8, width: 144, height: 48 },
            kind: CompiledNodeKind::Image(ImageSpec { image_id: "LOGO-ACME" }),
        };

        // Both kinds are decorative: no requirement, no static text — the startup
        // ScreenTextLayout and build()'s component derivation skip them.
        assert_eq!(PANEL.kind.requirement_id(), None);
        assert_eq!(PANEL.kind.text_key(), None);
        assert_eq!(IMAGE.kind.requirement_id(), None);
        assert_eq!(IMAGE.kind.text_key(), None);
    }

    #[test]
    fn status_indicator_spec_rejects_mismatched_or_empty_state_arrays() {
        const EMPTY: StatusIndicatorSpec = StatusIndicatorSpec {
            requirement_id: "REQ-X",
            source: "SRC",
            state_text_keys: &[],
            color_tokens: &[],
        };
        assert!(EMPTY.validate().is_err());

        const MISMATCHED: StatusIndicatorSpec = StatusIndicatorSpec {
            requirement_id: "REQ-X",
            source: "SRC",
            state_text_keys: &["STR-A", "STR-B"],
            color_tokens: &["Theme.Colors.Nominal"],
        };
        assert!(MISMATCHED.validate().is_err());

        const CONSISTENT: StatusIndicatorSpec = StatusIndicatorSpec {
            requirement_id: "REQ-X",
            source: "SRC",
            state_text_keys: &["STR-A", "STR-B"],
            color_tokens: &["Theme.Colors.Nominal", "Theme.Colors.Alert"],
        };
        assert!(CONSISTENT.validate().is_ok());
    }

    #[test]
    fn monitor_node_kinds_are_const_constructible_with_documented_accessors() {
        const MONITOR_SCREEN: CompiledScreenPackage = CompiledScreenPackage {
            screen_id: "MonitorKinds",
            layout: LayoutSpec {
                kind: LayoutKind::Vertical,
                spacing: 8,
                padding: 16,
            },
            nodes: &[
                CompiledNode {
                    id: "title",
                    bounds: Rect { x: 16, y: 16, width: 340, height: 48 },
                    kind: CompiledNodeKind::Label(LabelSpec {
                        text_key: "STR-NS-TITLE",
                        color_token: "Theme.Colors.Title",
                    }),
                },
                CompiledNode {
                    id: "wall-clock",
                    bounds: Rect { x: 372, y: 16, width: 400, height: 48 },
                    kind: CompiledNodeKind::Clock(ClockSpec {
                        format: ClockFormat::DateTimeSeconds,
                    }),
                },
                CompiledNode {
                    id: "sedation-index",
                    bounds: Rect { x: 16, y: 80, width: 400, height: 120 },
                    kind: CompiledNodeKind::NumericDisplay(NumericDisplaySpec {
                        requirement_id: "REQ-NS-001",
                        template_id: "TPL-SEDATION-INDEX",
                        source: "SEDATION_INDEX",
                        color_token: "Theme.Colors.ScoreDigits",
                    }),
                },
                CompiledNode {
                    id: "system-status",
                    bounds: Rect { x: 432, y: 80, width: 200, height: 48 },
                    kind: CompiledNodeKind::StatusIndicator(StatusIndicatorSpec {
                        requirement_id: "REQ-NS-003",
                        source: "MONITOR_STATUS",
                        state_text_keys: &["STR-NS-NOMINAL", "STR-NS-ALERT", "STR-NS-FAULT"],
                        color_tokens: &[
                            "Theme.Colors.Nominal",
                            "Theme.Colors.Alert",
                            "Theme.Colors.Fault",
                        ],
                    }),
                },
            ],
            golden_references: &[],
        };

        let label = MONITOR_SCREEN.find_node("title").expect("label exists");
        assert_eq!(label.kind.text_key(), Some("STR-NS-TITLE"));
        assert_eq!(label.kind.requirement_id(), None);

        let clock = MONITOR_SCREEN.find_node("wall-clock").expect("clock exists");
        assert_eq!(clock.kind.text_key(), None);
        assert_eq!(clock.kind.requirement_id(), None);

        let number = MONITOR_SCREEN
            .find_node("sedation-index")
            .expect("numeric display exists");
        assert_eq!(number.kind.text_key(), None);
        assert_eq!(number.kind.requirement_id(), Some("REQ-NS-001"));

        let status = MONITOR_SCREEN
            .find_node("system-status")
            .expect("status indicator exists");
        assert_eq!(status.kind.text_key(), None);
        assert_eq!(status.kind.requirement_id(), Some("REQ-NS-003"));
        if let CompiledNodeKind::StatusIndicator(spec) = status.kind {
            assert_eq!(spec.state_text_keys.len(), spec.color_tokens.len());
        } else {
            panic!("status node should be a StatusIndicator");
        }
    }

    #[test]
    fn button_and_text_input_kinds_are_const_constructible_with_documented_classifiers() {
        const ACK_BUTTON: CompiledNode = CompiledNode {
            id: "ack-button",
            bounds: Rect { x: 1392, y: 720, width: 240, height: 64 },
            kind: CompiledNodeKind::Button(ButtonSpec {
                text_key: "STR-NS-ACK",
                color_token: "Theme.Colors.PrimaryAction",
                source: "ACK_BUTTON",
                requirement_id: Some("REQ-NS-004"),
            }),
        };
        const DECORATIVE_BUTTON: CompiledNode = CompiledNode {
            id: "info-button",
            bounds: Rect { x: 0, y: 0, width: 240, height: 64 },
            kind: CompiledNodeKind::Button(ButtonSpec {
                text_key: "STR-INFO",
                color_token: "Theme.Colors.Neutral",
                source: "INFO_BUTTON",
                requirement_id: None,
            }),
        };
        const PATIENT_ID: CompiledNode = CompiledNode {
            id: "patient-id-input",
            bounds: Rect { x: 1392, y: 640, width: 512, height: 48 },
            kind: CompiledNodeKind::TextInput(TextInputSpec {
                source: "PATIENT_ID",
                max_length: 16,
                glyph_set_id: "SET-ASCII-TEXT",
                color_token: "Theme.Colors.Title",
                requirement_id: Some("REQ-NS-005"),
            }),
        };

        // Button renders a static approved label through the startup text path and is traced
        // exactly when it declares a requirement.
        assert_eq!(ACK_BUTTON.kind.text_key(), Some("STR-NS-ACK"));
        assert_eq!(ACK_BUTTON.kind.requirement_id(), Some("REQ-NS-004"));
        assert_eq!(DECORATIVE_BUTTON.kind.text_key(), Some("STR-INFO"));
        assert_eq!(DECORATIVE_BUTTON.kind.requirement_id(), None);

        // TextInput content is operator-typed and runtime-dynamic: no static text key, like
        // NumericDisplay; traced when it declares a requirement.
        assert_eq!(PATIENT_ID.kind.text_key(), None);
        assert_eq!(PATIENT_ID.kind.requirement_id(), Some("REQ-NS-005"));
    }

    #[test]
    fn button_spec_rejects_empty_fields() {
        const VALID: ButtonSpec = ButtonSpec {
            text_key: "STR-NS-ACK",
            color_token: "Theme.Colors.PrimaryAction",
            source: "ACK_BUTTON",
            requirement_id: None,
        };
        assert!(VALID.validate().is_ok());

        assert!(ButtonSpec { text_key: "", ..VALID }.validate().is_err());
        assert!(ButtonSpec { color_token: " ", ..VALID }.validate().is_err());
        assert!(ButtonSpec { source: "", ..VALID }.validate().is_err());
        // A declared requirement must be usable: whitespace-only fails here, not later at
        // RequirementId parsing during tracing.
        assert!(
            ButtonSpec { requirement_id: Some(" "), ..VALID }
                .validate()
                .is_err()
        );
        assert!(
            ButtonSpec { requirement_id: Some("REQ-NS-004"), ..VALID }
                .validate()
                .is_ok()
        );
    }

    #[test]
    fn text_input_spec_rejects_zero_capacity_and_empty_fields() {
        const VALID: TextInputSpec = TextInputSpec {
            source: "PATIENT_ID",
            max_length: 16,
            glyph_set_id: "SET-ASCII-TEXT",
            color_token: "Theme.Colors.Title",
            requirement_id: None,
        };
        assert!(VALID.validate().is_ok());

        assert_eq!(
            TextInputSpec { max_length: 0, ..VALID }
                .validate()
                .expect_err("zero capacity must be rejected")
                .to_string(),
            "text input max_length must be greater than zero"
        );
        assert!(TextInputSpec { source: "", ..VALID }.validate().is_err());
        assert!(TextInputSpec { glyph_set_id: " ", ..VALID }.validate().is_err());
        assert!(TextInputSpec { color_token: "", ..VALID }.validate().is_err());
        assert!(
            TextInputSpec { requirement_id: Some(" "), ..VALID }
                .validate()
                .is_err()
        );
        assert!(
            TextInputSpec { requirement_id: Some("REQ-NS-005"), ..VALID }
                .validate()
                .is_ok()
        );
    }

    #[test]
    fn finds_first_critical_button_in_compiled_screen() {
        const SCREEN: CompiledScreenPackage = CompiledScreenPackage {
            screen_id: "Hello",
            layout: LayoutSpec {
                kind: LayoutKind::Vertical,
                spacing: 8,
                padding: 16,
            },
            nodes: &[
                CompiledNode {
                    id: "viewport",
                    bounds: Rect {
                        x: 16,
                        y: 16,
                        width: 200,
                        height: 120,
                    },
                    kind: CompiledNodeKind::VulkanViewport(ViewportReservation {
                        stream_source: "STREAM",
                    }),
                },
                CompiledNode {
                    id: "button",
                    bounds: Rect {
                        x: 16,
                        y: 144,
                        width: 200,
                        height: 64,
                    },
                    kind: CompiledNodeKind::CriticalButton(CriticalButtonSpec {
                        requirement_id: "REQ-TEST-001",
                        text_key: "STR-HELLO-WORLD",
                        color_token: "Theme.Colors.PrimaryAction",
                        on_press: SystemEvent::NoOp,
                    }),
                },
            ],
            golden_references: &[],
        };

        let button = SCREEN
            .first_critical_button()
            .expect("critical button should exist");

        assert_eq!(button.id, "button");
        assert_eq!(
            button.kind.text_key(),
            Some("STR-HELLO-WORLD")
        );
    }
}
