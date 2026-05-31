use mdux::{
    ComplianceProgram, DeviceContext, FrameworkBuilder, GraphicsProfile, Hazard, Requirement,
    RequirementId, SafetyClass, UiComponent, UiSdkConfig, VerificationCase, VerificationMethod,
};

fn class_b_compliance_program() -> ComplianceProgram {
    let device = DeviceContext::new(
        "Acme Medical",
        "Infusion Pump",
        "therapy-ui",
        "0.1.0",
        SafetyClass::B,
    )
    .expect("device should be valid");

    let mut program = ComplianceProgram::new(device);
    let requirement_id = RequirementId::new("REQ-UI-001").expect("id should be valid");

    program.add_requirement(
        Requirement::new(
            requirement_id.clone(),
            "Display infusion rate",
            "IEC62304-5.2",
            "Verify the rendered therapy rate",
        )
        .expect("requirement should be valid"),
    );
    program.add_verification(
        VerificationCase::new(
            "VER-UI-001",
            requirement_id,
            VerificationMethod::Test,
            "Golden frame comparison",
        )
        .expect("verification should be valid"),
    );

    program
}

fn class_c_compliance_program() -> ComplianceProgram {
    let device = DeviceContext::new(
        "Acme Medical",
        "Ventilator",
        "alarm-ui",
        "0.1.0",
        SafetyClass::C,
    )
    .expect("device should be valid");

    let mut program = ComplianceProgram::new(device);
    let requirement_id = RequirementId::new("REQ-ALARM-001").expect("id should be valid");

    program.add_requirement(
        Requirement::new(
            requirement_id.clone(),
            "Render alarm state with deterministic priority",
            "IEC62304-5.3",
            "Verify deterministic alarm priority rendering",
        )
        .expect("requirement should be valid"),
    );
    program.add_hazard(
        Hazard::new(
            "HAZ-ALARM-001",
            "Alarm suppression due to non-deterministic UI update",
            vec![requirement_id.clone()],
        )
        .expect("hazard should be valid"),
    );
    program.add_verification(
        VerificationCase::new(
            "VER-ALARM-001",
            requirement_id,
            VerificationMethod::Test,
            "Offline deterministic frame trace",
        )
        .expect("verification should be valid"),
    );

    program
}

#[test]
fn builds_class_b_framework_and_exports_trace_matrix() {
    let device = DeviceContext::new(
        "Acme Medical",
        "Infusion Pump",
        "therapy-ui",
        "0.1.0",
        SafetyClass::B,
    )
    .expect("device should be valid");

    let requirement_id = RequirementId::new("REQ-UI-001").expect("id should be valid");
    let framework = FrameworkBuilder::new()
        .with_device(device)
        .with_compliance(class_b_compliance_program())
        .with_ui(UiSdkConfig::vulkan_class_b(1024, 600, 16))
        .add_component(
            UiComponent::new("screen-rate", "Rate Display", vec![requirement_id])
                .expect("component should be valid"),
        )
        .build()
        .expect("framework should build");

    let trace = framework.trace_matrix_export();
    let frame = framework.render_preview_frame(1);

    assert!(trace.contains("REQ-UI-001"));
    assert!(trace.contains("VER-UI-001"));
    assert_eq!(
        framework.ui_runtime().config().graphics_profile,
        GraphicsProfile::Vulkan
    );
    assert!(frame.dynamic_allocations > 0);
    assert!(
        framework
            .audit_export()
            .contains("framework build completed")
    );
}

#[test]
fn builds_class_c_vulkansc_framework_with_zero_runtime_allocations() {
    let device = DeviceContext::new(
        "Acme Medical",
        "Ventilator",
        "alarm-ui",
        "0.1.0",
        SafetyClass::C,
    )
    .expect("device should be valid");

    let requirement_id = RequirementId::new("REQ-ALARM-001").expect("id should be valid");
    let framework = FrameworkBuilder::new()
        .with_device(device)
        .with_compliance(class_c_compliance_program())
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

    let frame = framework.render_preview_frame(7);

    assert_eq!(
        framework.ui_runtime().config().graphics_profile,
        GraphicsProfile::VulkanSc
    );
    assert_eq!(frame.dynamic_allocations, 0);
    assert!(framework.release_summary().contains("Class C"));
}

#[test]
fn class_c_requires_vulkansc() {
    let device = DeviceContext::new(
        "Acme Medical",
        "Ventilator",
        "alarm-ui",
        "0.1.0",
        SafetyClass::C,
    )
    .expect("device should be valid");

    let requirement_id = RequirementId::new("REQ-ALARM-001").expect("id should be valid");
    let result = FrameworkBuilder::new()
        .with_device(device)
        .with_compliance(class_c_compliance_program())
        .with_ui(UiSdkConfig::vulkan_class_b(1280, 720, 12))
        .add_component(
            UiComponent::new("screen-alarm", "Alarm Banner", vec![requirement_id])
                .expect("component should be valid"),
        )
        .build();

    let error = match result {
        Err(error) => error,
        Ok(_) => panic!("Class C without Vulkan SC should fail"),
    };

    assert_eq!(
        error.to_string(),
        "Class C devices must use the Vulkan SC graphics profile"
    );
}
