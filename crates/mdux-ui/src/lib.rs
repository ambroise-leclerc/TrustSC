#![forbid(unsafe_code)]

use mdux_core::{DeterminismPolicy, MduxResult, ValidationError, Validates};
use mdux_governance::RequirementId;

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

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum CompiledNodeKind {
    CriticalButton(CriticalButtonSpec),
    VulkanViewport(ViewportReservation),
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
    pub fn requirement_id(&self) -> Option<&'static str> {
        match self {
            Self::CriticalButton(specification) => Some(specification.requirement_id),
            Self::VulkanViewport(_) => None,
        }
    }

    pub fn text_key(&self) -> Option<&'static str> {
        match self {
            Self::CriticalButton(specification) => Some(specification.text_key),
            Self::VulkanViewport(_) => None,
        }
    }
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
        let estimated_frame_time_ms = (draw_calls.max(1) * 2)
            .min(self.config.determinism_policy.max_frame_time_ms);

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
