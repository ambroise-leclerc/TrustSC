#![forbid(unsafe_code)]

mod standard_text;

pub use mdux_core::{
    DeterminismPolicy, DeviceContext, FrameworkIdentity, MduxResult, SafetyClass, ValidationError,
};
pub use mdux_governance::{
    AuditCategory, AuditEvent, ComplianceProgram, Hazard, ProblemReport, Requirement, RequirementId,
    VerificationCase, VerificationMethod,
};
pub use mdux_text_authoring::{
    compile_text_package, fingerprint_font_file, DeterministicAtlasBuilder, FontFingerprint,
    RasterizedGlyph, TextCompilationInput,
};
pub use mdux_text_runtime::{GlyphDrawCommand, TextRuntime};
pub use mdux_text_schema::{
    ApprovedString, AtlasGlyph, CompiledGlyph, CompiledTextRun, DeterminismEvidence, FontAsset,
    NumericGlyphEntry, NumericGlyphSet, NumericTemplate, TextDirection, TextPackage, TextureAtlas,
};
pub use mdux_ui::{
    CompiledNode, CompiledNodeKind, CompiledScreenPackage, CriticalButtonSpec, CvCheckKind,
    FrameStatistics, GoldenReferenceEntry, GraphicsProfile, LayoutKind, LayoutSpec,
    MedicalUiRuntime, PipelineMode, Rect, SystemEvent, UiComponent, UiSdkConfig,
    ViewportReservation,
};
pub use standard_text::{
    default_standard_text_package, StandardFontDefinition, DEFAULT_STANDARD_FONT,
    DEFAULT_STANDARD_FONT_SOURCE_PATH, DEFAULT_STANDARD_HELLO_WORLD_RUN_ID,
    DEFAULT_STANDARD_HELLO_WORLD_STRING_ID, DEFAULT_STANDARD_HELLO_WORLD_TEXT,
    ROBOTO_REGULAR_400_16PX,
};

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct HelloWorldDemoConfig {
    pub manufacturer: String,
    pub product_name: String,
    pub software_item: String,
    pub version: String,
    pub greeting: String,
    pub width: u32,
    pub height: u32,
    pub max_frame_time_ms: u32,
}

impl Default for HelloWorldDemoConfig {
    fn default() -> Self {
        Self {
            manufacturer: "Acme Medical".to_string(),
            product_name: "MduX-rust Hello World".to_string(),
            software_item: "hello-world-ui".to_string(),
            version: "0.1.0".to_string(),
            greeting: DEFAULT_STANDARD_HELLO_WORLD_TEXT.to_string(),
            width: 800,
            height: 480,
            max_frame_time_ms: 16,
        }
    }
}

pub struct HelloWorldDemoRun {
    pub framework: Framework,
    pub frame: FrameStatistics,
}

pub fn build_hello_world_demo(config: HelloWorldDemoConfig) -> MduxResult<Framework> {
    let device = DeviceContext::new(
        config.manufacturer,
        config.product_name,
        config.software_item,
        config.version,
        SafetyClass::B,
    )?;
    let requirement_id = RequirementId::new("REQ-HELLO-001")?;

    let mut compliance = ComplianceProgram::new(device.clone());
    compliance.add_requirement(Requirement::new(
        requirement_id.clone(),
        "Render the hello-world greeting",
        "IEC62304-5.2",
        "Verify the smoke demo renders a greeting component",
    )?);
    compliance.add_verification(VerificationCase::new(
        "VER-HELLO-001",
        requirement_id.clone(),
        VerificationMethod::Test,
        "Preview frame execution in the host smoke demo",
    )?);

    FrameworkBuilder::new()
        .with_device(device)
        .with_compliance(compliance)
        .with_ui(UiSdkConfig::vulkan_class_b(
            config.width,
            config.height,
            config.max_frame_time_ms,
        ))
        .add_component(UiComponent::new(
            "hello-world-label",
            config.greeting,
            vec![requirement_id],
        )?)
        .build()
}

pub fn run_hello_world_demo(config: HelloWorldDemoConfig) -> MduxResult<HelloWorldDemoRun> {
    let framework = build_hello_world_demo(config)?;
    let frame = framework.render_preview_frame(1);

    Ok(HelloWorldDemoRun { framework, frame })
}

pub struct FrameworkBuilder {
    device: Option<DeviceContext>,
    compliance: Option<ComplianceProgram>,
    ui_config: Option<UiSdkConfig>,
    ui_components: Vec<UiComponent>,
}

impl FrameworkBuilder {
    pub fn new() -> Self {
        Self {
            device: None,
            compliance: None,
            ui_config: None,
            ui_components: Vec::new(),
        }
    }

    pub fn with_device(mut self, device: DeviceContext) -> Self {
        self.device = Some(device);
        self
    }

    pub fn with_compliance(mut self, compliance: ComplianceProgram) -> Self {
        self.compliance = Some(compliance);
        self
    }

    pub fn with_ui(mut self, ui_config: UiSdkConfig) -> Self {
        self.ui_config = Some(ui_config);
        self
    }

    pub fn add_component(mut self, component: UiComponent) -> Self {
        self.ui_components.push(component);
        self
    }

    pub fn build(self) -> MduxResult<Framework> {
        let device = self
            .device
            .ok_or_else(|| ValidationError::new("framework builder requires a device context"))?;
        let mut compliance = self
            .compliance
            .ok_or_else(|| ValidationError::new("framework builder requires a compliance program"))?;
        let ui_config = self
            .ui_config
            .ok_or_else(|| ValidationError::new("framework builder requires a ui configuration"))?;

        if compliance.device() != &device {
            return Err(ValidationError::new(
                "device context and compliance program must describe the same software item",
            ));
        }

        compliance.validate()?;

        if device.safety_class == SafetyClass::C
            && ui_config.graphics_profile != GraphicsProfile::VulkanSc
        {
            return Err(ValidationError::new(
                "Class C devices must use the Vulkan SC graphics profile",
            ));
        }

        for component in &self.ui_components {
            for requirement_id in &component.requirement_ids {
                if !compliance.has_requirement(requirement_id) {
                    return Err(ValidationError::new(format!(
                        "ui component {} references unknown requirement {}",
                        component.id, requirement_id
                    )));
                }
            }
        }

        let ui = MedicalUiRuntime::new(ui_config, self.ui_components)?;

        compliance.record_event(
            AuditCategory::Release,
            format!("framework build completed for {}", device.software_item),
        );

        Ok(Framework {
            identity: FrameworkIdentity::default(),
            device,
            compliance,
            ui,
        })
    }
}

impl Default for FrameworkBuilder {
    fn default() -> Self {
        Self::new()
    }
}

pub struct Framework {
    identity: FrameworkIdentity,
    device: DeviceContext,
    compliance: ComplianceProgram,
    ui: MedicalUiRuntime,
}

impl Framework {
    pub fn identity(&self) -> &FrameworkIdentity {
        &self.identity
    }

    pub fn device(&self) -> &DeviceContext {
        &self.device
    }

    pub fn ui_runtime(&self) -> &MedicalUiRuntime {
        &self.ui
    }

    pub fn trace_matrix_export(&self) -> String {
        self.compliance.trace_matrix_export()
    }

    pub fn audit_export(&self) -> String {
        self.compliance.audit_export()
    }

    pub fn release_summary(&self) -> String {
        format!(
            "framework={} version={} profile={} components={} {}",
            self.identity.name,
            self.identity.version,
            self.ui.config().profile_name(),
            self.ui.components().len(),
            self.compliance.release_evidence_summary()
        )
    }

    pub fn render_preview_frame(&self, frame_index: u64) -> FrameStatistics {
        self.ui.render_frame(frame_index)
    }
}
