//! Replays every compiled scenario (ADR-016 §4) against the same screen bindings and the same
//! `AppLogic` closures the windowed binary registers — a plain `cargo test`, no GPU, no Vulkan.

mdux::include_medui_screen!();
mdux::include_scenarios!();

use class_c_monitor::app_logic::AppLogic;
use mdux::input::FrameEvents;
use mdux::realtime::{FrameInputs, ScreenBindings};
use mdux::verify_scenario::run_scenario;

#[test]
fn every_compiled_scenario_passes() {
    let screen = medui_screen::screen();
    let standard = mdux::default_standard_text_package().expect("standard text package");
    let displays = mdux::default_display_text_packages().expect("display text packages");
    let images = mdux::default_image_packages().expect("image packages");
    let bindings = ScreenBindings::from_screen(screen, standard, displays, &images, "en-US")
        .expect("screen bindings resolve");

    assert!(
        !verify_scenarios::SCENARIOS.is_empty(),
        "expected at least one compiled scenario"
    );

    for scenario in verify_scenarios::SCENARIOS {
        // Fresh application state and a fresh event queue per scenario: one scenario's replay
        // must never leak into another's starting conditions.
        let mut frame_inputs = FrameInputs::from_bindings(&bindings).expect("frame inputs");
        let mut events = FrameEvents::new();
        let (mut input, mut realtime) = AppLogic::new().into_closures();

        let trace = run_scenario(
            scenario,
            &mut events,
            &mut frame_inputs,
            &mut input,
            &mut realtime,
            |_label, _frame_inputs| {},
        );

        assert!(
            trace.passed,
            "scenario {:?} failed:\n{:#?}",
            scenario.id, trace.steps
        );
    }
}
