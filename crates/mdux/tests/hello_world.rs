use mdux::{
    ComplianceProgram, CompiledNode, CompiledNodeKind, CompiledScreenPackage, CriticalButtonSpec,
    DEFAULT_STANDARD_HELLO_WORLD_TEXT, DeviceContext, FrameworkBuilder, GraphicsProfile,
    LayoutKind, LayoutSpec, Rect, Requirement, RequirementId, SafetyClass, SystemEvent,
    UiComponent, UiSdkConfig, VerificationCase, VerificationMethod,
};

const SCREEN: CompiledScreenPackage = CompiledScreenPackage {
    screen_id: "HelloWorld",
    layout: LayoutSpec {
        kind: LayoutKind::Vertical,
        spacing: 16,
        padding: 24,
    },
    nodes: &[CompiledNode {
        id: "hello-world-label",
        bounds: Rect {
            x: 24,
            y: 24,
            width: 400,
            height: 80,
        },
        kind: CompiledNodeKind::CriticalButton(CriticalButtonSpec {
            requirement_id: "REQ-HELLO-001",
            text_key: "STR-HELLO-WORLD",
            color_token: "Theme.Colors.PrimaryAction",
            on_press: SystemEvent::NoOp,
        }),
    }],
    golden_references: &[],
};

#[test]
fn builds_framework_from_screen_through_public_api() {
    let device = DeviceContext::new(
        "Acme Medical",
        "MduX-rust Hello World",
        "hello-world-ui",
        "0.1.0",
        SafetyClass::B,
    )
    .expect("device context should validate");
    let requirement_id = RequirementId::new("REQ-HELLO-001").expect("requirement id should parse");

    let mut compliance = ComplianceProgram::new(device.clone());
    compliance.add_requirement(
        Requirement::new(
            requirement_id.clone(),
            "Render the hello-world greeting",
            "IEC62304-5.2",
            "Verify the smoke demo renders a greeting component",
        )
        .expect("requirement should validate"),
    );
    compliance.add_verification(
        VerificationCase::new(
            "VER-HELLO-001",
            requirement_id,
            VerificationMethod::Test,
            "Preview frame execution in the host smoke demo",
        )
        .expect("verification case should validate"),
    );

    let framework = FrameworkBuilder::new()
        .with_device(device)
        .with_compliance(compliance)
        .with_ui(UiSdkConfig::vulkan_class_b(800, 480, 16))
        .with_screen(&SCREEN)
        .build()
        .expect("framework should build from a compiled screen package");

    let frame = framework.render_preview_frame(1);

    assert_eq!(
        framework.ui_runtime().config().graphics_profile,
        GraphicsProfile::Vulkan
    );
    assert_eq!(frame.frame_index, 1);
    assert!(frame.draw_calls > 0);
    assert!(framework.release_summary().contains("hello-world-ui"));
    assert!(framework.trace_matrix_export().contains("REQ-HELLO-001"));
    assert!(
        framework
            .audit_export()
            .contains("framework build completed")
    );
    assert_eq!(
        framework.ui_runtime().components()[0].label,
        DEFAULT_STANDARD_HELLO_WORLD_TEXT
    );
    assert_eq!(framework.ui_runtime().components()[0].id, "hello-world-label");
}

fn hello_world_builder() -> FrameworkBuilder {
    let device = DeviceContext::new(
        "Acme Medical",
        "MduX-rust Hello World",
        "hello-world-ui",
        "0.1.0",
        SafetyClass::B,
    )
    .expect("device context should validate");
    let requirement_id = RequirementId::new("REQ-HELLO-001").expect("requirement id should parse");

    let mut compliance = ComplianceProgram::new(device.clone());
    compliance.add_requirement(
        Requirement::new(
            requirement_id.clone(),
            "Render the hello-world greeting",
            "IEC62304-5.2",
            "Verify the smoke demo renders a greeting component",
        )
        .expect("requirement should validate"),
    );
    compliance.add_verification(
        VerificationCase::new(
            "VER-HELLO-001",
            requirement_id,
            VerificationMethod::Test,
            "Preview frame execution in the host smoke demo",
        )
        .expect("verification case should validate"),
    );

    FrameworkBuilder::new()
        .with_device(device)
        .with_compliance(compliance)
        .with_ui(UiSdkConfig::vulkan_class_b(800, 480, 16))
}

#[test]
fn with_screen_locale_rejects_empty_locale() {
    let error = hello_world_builder()
        .with_screen(&SCREEN)
        .with_screen_locale("   ")
        .build()
        .err()
        .expect("empty screen locale should be rejected");

    assert!(error.to_string().contains("locale"));
}

#[test]
fn build_rejects_duplicate_ui_component_ids() {
    let error = hello_world_builder()
        .add_component(
            UiComponent::new(
                "hello-world-label",
                "duplicate",
                vec![RequirementId::new("REQ-HELLO-001").expect("requirement id should parse")],
            )
            .expect("component should validate"),
        )
        .with_screen(&SCREEN)
        .build()
        .err()
        .expect("duplicate component ids across add_component and with_screen should be rejected");

    assert!(error.to_string().contains("duplicate"));
    assert!(error.to_string().contains("hello-world-label"));
}
