#![forbid(unsafe_code)]

use trustsc::{
    ComplianceProgram, DeviceContext, FrameworkBuilder, Requirement, RequirementId, SafetyClass,
    UiComponent, UiSdkConfig, VerificationCase, VerificationMethod,
};

fn main() {
    let device = DeviceContext::new(
        "Acme Medical",
        "Infusion Pump",
        "therapy-ui",
        "0.1.0",
        SafetyClass::B,
    )
    .expect("device should be valid");

    let requirement_id = RequirementId::new("REQ-THERAPY-001").expect("id should be valid");
    let mut compliance = ComplianceProgram::new(device.clone());
    compliance.add_requirement(
        Requirement::new(
            requirement_id.clone(),
            "Display active therapy rate",
            "IEC62304-5.2",
            "Verify nominal therapy display output",
        )
        .expect("requirement should be valid"),
    );
    compliance.add_verification(
        VerificationCase::new(
            "VER-THERAPY-001",
            requirement_id.clone(),
            VerificationMethod::Test,
            "Golden frame regression",
        )
        .expect("verification should be valid"),
    );

    let framework = FrameworkBuilder::new()
        .with_device(device)
        .with_compliance(compliance)
        .with_ui(UiSdkConfig::vulkan_class_b(1024, 600, 16))
        .add_component(
            UiComponent::new("screen-main", "Therapy Screen", vec![requirement_id])
                .expect("component should be valid"),
        )
        .build()
        .expect("framework should build");

    println!("{}", framework.release_summary());
    println!("{}", framework.trace_matrix_export());
}
