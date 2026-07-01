mdux::include_medui_screen!();

use mdux::{
    ComplianceProgram, DeviceContext, FrameworkBuilder, Requirement, RequirementId, SafetyClass,
    UiSdkConfig, VerificationCase, VerificationMethod,
};

fn main() -> Result<(), Box<dyn std::error::Error>> {
    let device = DeviceContext::new(
        "Acme Medical",
        "MduX-rust Hello World",
        "hello-world-ui",
        "0.1.0",
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
        requirement_id,
        VerificationMethod::Test,
        "Preview frame execution in the host smoke demo",
    )?);

    let framework = FrameworkBuilder::new()
        .with_device(device)
        .with_compliance(compliance)
        .with_ui(UiSdkConfig::vulkan_class_b(800, 480, 16))
        .with_screen(medui_screen::screen())
        .build()?;

    mdux_vulkan_winit::App::new(framework, medui_screen::screen()).run_from_env()
}
