mdux::include_medui_screen!();
mdux::include_scenarios!();

use class_c_monitor::app_logic::AppLogic;
use mdux::{
    ComplianceProgram, DeviceContext, FrameworkBuilder, Hazard, Requirement, RequirementId,
    SafetyClass, UiSdkConfig, VerificationCase, VerificationMethod,
};

fn main() -> Result<(), Box<dyn std::error::Error>> {
    let device = DeviceContext::new(
        "Acme Medical",
        "NeuroSense 500",
        "neurosense-ui",
        "0.1.0",
        SafetyClass::C,
    )?;

    let mut compliance = ComplianceProgram::new(device.clone());
    let req_index = RequirementId::new("REQ-NS-001")?;
    let req_stream = RequirementId::new("REQ-NS-002")?;
    let req_status = RequirementId::new("REQ-NS-003")?;
    let req_ack = RequirementId::new("REQ-NS-004")?;
    let req_patient_id = RequirementId::new("REQ-NS-005")?;
    for (id, verification_id, title) in [
        (&req_index, "VER-NS-001", "Display the sedation index, refreshed every frame"),
        (&req_stream, "VER-NS-002", "Display the spectral stream with visible freshness"),
        (&req_status, "VER-NS-003", "Keep the system status permanently visible"),
        (&req_ack, "VER-NS-004", "Capture operator acknowledgment of the active alert"),
        (
            &req_patient_id,
            "VER-NS-005",
            "Bound patient identifier entry to the approved character set and length",
        ),
    ] {
        compliance.add_requirement(Requirement::new(
            id.clone(),
            title,
            "IEC62304-5.2",
            "Verified by windowed demonstration and headless smoke",
        )?);
        compliance.add_verification(VerificationCase::new(
            verification_id,
            id.clone(),
            VerificationMethod::Demonstration,
            "Windowed run on the development host",
        )?);
    }
    compliance.add_hazard(Hazard::new(
        "HAZ-NS-001",
        "A stale or frozen sedation index misleads the anesthesiologist",
        vec![req_index, req_stream],
    )?);

    let screen = medui_screen::screen();
    let framework = FrameworkBuilder::new()
        .with_device(device)
        .with_compliance(compliance)
        .with_ui(UiSdkConfig::vulkansc_class_c(
            medui_screen::GENERATED_MEDUI_SURFACE.0,
            medui_screen::GENERATED_MEDUI_SURFACE.1,
            12,
            32 * 1024 * 1024,
            256,
        ))
        .with_screen(screen)
        .build()?;

    // The interaction/realtime state and the two closures registered below are shared, verbatim,
    // with the scenario test (tests/scenarios.rs) through the class_c_monitor library crate —
    // there is no second, test-only implementation of this behavior.
    let (input, realtime) = AppLogic::new().into_closures();

    mdux_vulkan_winit::App::new(framework, screen)
        .with_input(input)
        .with_realtime(realtime)
        .run_from_env()
}
