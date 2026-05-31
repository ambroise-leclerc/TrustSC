use mdux::{
    run_hello_world_demo, GraphicsProfile, HelloWorldDemoConfig, DEFAULT_STANDARD_HELLO_WORLD_TEXT,
};

#[test]
fn builds_hello_world_demo_through_public_api() {
    assert_eq!(
        HelloWorldDemoConfig::default().greeting,
        DEFAULT_STANDARD_HELLO_WORLD_TEXT
    );

    let run = run_hello_world_demo(HelloWorldDemoConfig::default())
        .expect("hello world demo should build and run");

    assert_eq!(
        run.framework.ui_runtime().config().graphics_profile,
        GraphicsProfile::Vulkan
    );
    assert_eq!(run.frame.frame_index, 1);
    assert!(run.frame.draw_calls > 0);
    assert!(run.frame.dynamic_allocations > 0);
    assert!(run.framework.release_summary().contains("hello-world-ui"));
    assert!(run.framework.trace_matrix_export().contains("REQ-HELLO-001"));
    assert!(run.framework.audit_export().contains("framework build completed"));
    assert_eq!(
        run.framework.ui_runtime().components()[0].label,
        DEFAULT_STANDARD_HELLO_WORLD_TEXT
    );
}
