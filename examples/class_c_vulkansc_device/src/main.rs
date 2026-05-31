#![forbid(unsafe_code)]

use mdux::{
    ComplianceProgram, DeviceContext, FrameworkBuilder, Hazard, Requirement, RequirementId,
    SafetyClass, UiComponent, UiSdkConfig, VerificationCase, VerificationMethod,
};

fn main() {
    let device = DeviceContext::new(
        "Acme Medical",
        "Ventilator",
        "alarm-ui",
        "0.1.0",
        SafetyClass::C,
    )
    .expect("device should be valid");

    let requirement_id = RequirementId::new("REQ-ALARM-001").expect("id should be valid");
    let mut compliance = ComplianceProgram::new(device.clone());
    compliance.add_requirement(
        Requirement::new(
            requirement_id.clone(),
            "Render alarm priority deterministically",
            "IEC62304-5.3",
            "Verify deterministic alarm rendering under load",
        )
        .expect("requirement should be valid"),
    );
    compliance.add_hazard(
        Hazard::new(
            "HAZ-ALARM-001",
            "Alarm banner suppression caused by non-deterministic state transition",
            vec![requirement_id.clone()],
        )
        .expect("hazard should be valid"),
    );
    compliance.add_verification(
        VerificationCase::new(
            "VER-ALARM-001",
            requirement_id.clone(),
            VerificationMethod::Test,
            "Offline pipeline trace and reserved-memory frame replay",
        )
        .expect("verification should be valid"),
    );

    let framework = FrameworkBuilder::new()
        .with_device(device)
        .with_compliance(compliance)
        .with_ui(UiSdkConfig::vulkansc_class_c(
            1280,
            720,
            12,
            512 * 1024,
            128,
        ))
        .add_component(
            UiComponent::new("screen-alarm", "Alarm Banner", vec![requirement_id])
                .expect("component should be valid"),
        )
        .build()
        .expect("framework should build");

    println!("{}", framework.release_summary());
    println!("{}", framework.audit_export());
}
