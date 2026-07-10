//! The scenario plane (ADR-016 §4): authored TOML behavior scripts, compiled by `mdux-build`
//! into `&'static` data — the ADR-008 doctrine applied to interaction scripts, the same way
//! `.medui` compiles to a `CompiledScreenPackage`. The types here are deliberately
//! const-constructible (every field is `&'static str`/numeric/a `&'static` slice) so a generated
//! module can declare a `ScenarioScript` as a plain static, no allocation, no runtime parsing.
//!
//! [`run_scenario`] is the GPU-free half of the story: it replays a script's steps through the
//! *application's own* `with_input`/`with_realtime` closures (ADR-015's `FrameEvents` →
//! `FrameInputs` plane), asserting the echoed state and recording an event → expected → observed
//! trace for every step. `Capture` steps carry no assertion here — they are a named checkpoint
//! wave W6's offscreen renderer hooks into via `on_capture`; this runner only guarantees the
//! capture happens against the same settled state a real redraw would see.

use crate::input::{FrameEvents, WidgetEvent};
use crate::realtime::FrameInputs;

/// The wall-clock date/time a scenario pins for `Clock` nodes. Carried through to the trace for
/// wave W6's offscreen capture path; the GPU-free runner never reads it (there is no `Clock`
/// node to render without a screen).
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct ScenarioClock {
    pub year: u16,
    pub month: u8,
    pub day: u8,
    pub hours: u8,
    pub minutes: u8,
    pub seconds: u8,
}

/// One scripted step. `Event` queues an ADR-015 `WidgetEvent` for the next settled frame;
/// `Advance` forces `frames` additional input+realtime settles right away (e.g. to fill a
/// sliding-window classifier's buffer one sample per tick — ADR-017/018's ML-driven examples need
/// this to reach a real depth-of-anesthesia state deterministically);
/// `ExpectText`/`ExpectStatus`/`ExpectNumber` assert against `FrameInputs`; `Capture` names a
/// checkpoint for the offscreen renderer.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum ScenarioStep {
    Event(WidgetEvent),
    Advance { frames: u32 },
    ExpectText { source: &'static str, value: &'static str },
    ExpectStatus { source: &'static str, value: u8 },
    ExpectNumber { source: &'static str, value: i64 },
    Capture { label: &'static str },
}

/// A compiled behavior script: authored TOML in, static data out (mirrors `CompiledScreenPackage`
/// for the `.medui` DSL). `requirement_ids` ties the scenario to the requirements it demonstrates,
/// for the evidence trace join wave W6/§5 builds.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct ScenarioScript {
    pub id: &'static str,
    pub requirement_ids: &'static [&'static str],
    pub clock: ScenarioClock,
    pub steps: &'static [ScenarioStep],
}

/// One step's evidence: what was scripted, what was expected (if anything), what was actually
/// observed, and whether the step passed. `Capture` steps always pass — there is nothing to
/// assert. `Event` steps pass unless the bounded `FrameEvents` queue was full and silently
/// dropped them (ADR-015); either way every step lands in the trace, so the full
/// event → expected → observed sequence is reconstructable from `ScenarioTrace::steps` alone.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct StepTrace {
    pub index: usize,
    pub description: String,
    pub expected: Option<String>,
    pub observed: Option<String>,
    pub passed: bool,
}

/// The full replay trace of one scenario. `passed` is the conjunction of every step's `passed`;
/// a failed expectation never stops the replay, so a single run surfaces every divergence rather
/// than just the first one.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct ScenarioTrace {
    pub scenario_id: &'static str,
    pub steps: Vec<StepTrace>,
    pub passed: bool,
    /// Labels of `Capture` steps, in script order — wave W6's offscreen harness walks this to
    /// know which named frames to render, without re-deriving it from `steps`.
    pub capture_labels: Vec<&'static str>,
}

/// Replays `script` against the application's own registered closures with no GPU and no
/// offscreen rendering. Mirrors the presentation adapter's per-frame order exactly (see
/// `adapters/mdux-vulkan-winit`'s `RedrawRequested` handler): the input closure drains
/// `FrameEvents` and echoes state first, then the realtime closure runs — there is no second,
/// diverging "test" ordering.
///
/// **Frame-batching semantics.** An `Event` step only enqueues into `events`; it never runs a
/// frame by itself. Consecutive `Event` steps therefore batch: `Event, Event, ExpectText` drains
/// both queued events in a single frame immediately before the assertion, exactly like a burst of
/// real keystrokes settling before the next redraw. A frame runs exactly once, lazily, the moment
/// an `Expect*`/`Capture` step needs settled state that hasn't been produced yet; once that frame
/// has run, further `Expect*`/`Capture` steps in a row read the *same* settled state without
/// re-invoking `realtime_closure` again, until another `Event` step queues new work. A frame also
/// runs before the very first expectation/capture even if no `Event` preceded it, so a scenario
/// can assert the screen's initial state.
///
/// **`Advance` is the one exception to laziness.** Unlike every other step, `Advance { frames }`
/// runs `frames` input+realtime settles immediately, unconditionally — it exists specifically to
/// drive time-dependent realtime state (a deterministic simulator's tick counter, a sliding
/// window filling up one sample per frame) forward by an exact count, which a scenario cannot
/// express through `Event`/`Expect*` alone since those only ever settle *one* frame.
///
/// `capture` steps carry no pass/fail of their own; `on_capture` is invoked with the step's label
/// and the settled `FrameInputs` so a caller (a test, or wave W6's offscreen renderer) can inspect
/// or render at that point.
pub fn run_scenario(
    script: &ScenarioScript,
    events: &mut FrameEvents,
    frame_inputs: &mut FrameInputs,
    input_closure: &mut dyn FnMut(&mut FrameEvents, &mut FrameInputs),
    realtime_closure: &mut dyn FnMut(&mut FrameInputs),
    mut on_capture: impl FnMut(&str, &FrameInputs),
) -> ScenarioTrace {
    let mut steps = Vec::with_capacity(script.steps.len());
    let mut capture_labels = Vec::new();
    let mut passed = true;
    // A frame must settle before the very first expectation/capture even when no `Event` step
    // precedes it (asserting the screen's initial state), so this starts true.
    let mut needs_frame = true;

    for (index, step) in script.steps.iter().enumerate() {
        match *step {
            ScenarioStep::Event(event) => {
                // FrameEvents is a bounded queue (ADR-015): a dropped event must fail the step
                // rather than let the scenario "pass" while silently never delivering the
                // scripted input.
                let queued = events.push(event);
                needs_frame = true;
                steps.push(StepTrace {
                    index,
                    description: format!("event {event:?}"),
                    expected: None,
                    observed: if queued {
                        None
                    } else {
                        Some("dropped: FrameEvents queue is full".to_string())
                    },
                    passed: queued,
                });
                passed &= queued;
            }
            ScenarioStep::Advance { frames } => {
                for _ in 0..frames {
                    input_closure(events, frame_inputs);
                    realtime_closure(frame_inputs);
                    needs_frame = false;
                }
                steps.push(StepTrace {
                    index,
                    description: format!("advance frames={frames}"),
                    expected: None,
                    observed: None,
                    passed: true,
                });
            }
            ScenarioStep::ExpectText { source, value } => {
                settle_frame(&mut needs_frame, events, frame_inputs, input_closure, realtime_closure);
                let observed = frame_inputs.text(source).map(str::to_string);
                let step_passed = observed.as_deref() == Some(value);
                passed &= step_passed;
                steps.push(StepTrace {
                    index,
                    description: format!("expect_text source={source}"),
                    expected: Some(value.to_string()),
                    observed,
                    passed: step_passed,
                });
            }
            ScenarioStep::ExpectStatus { source, value } => {
                settle_frame(&mut needs_frame, events, frame_inputs, input_closure, realtime_closure);
                let observed = frame_inputs.status_index(source);
                let step_passed = observed == Some(value);
                passed &= step_passed;
                steps.push(StepTrace {
                    index,
                    description: format!("expect_status source={source}"),
                    expected: Some(value.to_string()),
                    observed: observed.map(|value| value.to_string()),
                    passed: step_passed,
                });
            }
            ScenarioStep::ExpectNumber { source, value } => {
                settle_frame(&mut needs_frame, events, frame_inputs, input_closure, realtime_closure);
                let observed = frame_inputs.number(source);
                let step_passed = observed == Some(value);
                passed &= step_passed;
                steps.push(StepTrace {
                    index,
                    description: format!("expect_number source={source}"),
                    expected: Some(value.to_string()),
                    observed: observed.map(|value| value.to_string()),
                    passed: step_passed,
                });
            }
            ScenarioStep::Capture { label } => {
                settle_frame(&mut needs_frame, events, frame_inputs, input_closure, realtime_closure);
                on_capture(label, frame_inputs);
                capture_labels.push(label);
                steps.push(StepTrace {
                    index,
                    description: format!("capture {label}"),
                    expected: None,
                    observed: None,
                    passed: true,
                });
            }
        }
    }

    ScenarioTrace {
        scenario_id: script.id,
        steps,
        passed,
        capture_labels,
    }
}

/// Runs exactly one frame (input closure, then realtime closure — ADR-015 order) when
/// `*needs_frame` is set, then clears the flag. A no-op when the current state is already
/// settled, which is what lets consecutive `Expect*`/`Capture` steps share one frame's output.
fn settle_frame(
    needs_frame: &mut bool,
    events: &mut FrameEvents,
    frame_inputs: &mut FrameInputs,
    input_closure: &mut dyn FnMut(&mut FrameEvents, &mut FrameInputs),
    realtime_closure: &mut dyn FnMut(&mut FrameInputs),
) {
    if !*needs_frame {
        return;
    }
    input_closure(events, frame_inputs);
    realtime_closure(frame_inputs);
    *needs_frame = false;
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::realtime::ScreenBindings;
    use crate::{
        ButtonSpec, CompiledNode, CompiledNodeKind, CompiledScreenPackage, LayoutKind, LayoutSpec,
        Rect, StatusIndicatorSpec, TextInputModel, TextInputSpec, default_display_text_packages,
        default_standard_text_package,
    };

    const TEST_SCREEN: CompiledScreenPackage = CompiledScreenPackage {
        screen_id: "ScenarioRunnerTest",
        layout: LayoutSpec {
            kind: LayoutKind::Vertical,
            spacing: 8,
            padding: 16,
        },
        nodes: &[
            CompiledNode {
                id: "patient-id-input",
                bounds: Rect { x: 16, y: 16, width: 512, height: 48 },
                kind: CompiledNodeKind::TextInput(TextInputSpec {
                    source: "PATIENT_ID",
                    max_length: 16,
                    glyph_set_id: "SET-ASCII-TEXT",
                    color_token: "Theme.Colors.Title",
                    requirement_id: None,
                }),
            },
            CompiledNode {
                id: "ack-button",
                bounds: Rect { x: 16, y: 80, width: 240, height: 64 },
                kind: CompiledNodeKind::Button(ButtonSpec {
                    text_key: "STR-NS-ACK",
                    color_token: "Theme.Colors.PrimaryAction",
                    source: "ACK_BUTTON",
                    requirement_id: None,
                }),
            },
            CompiledNode {
                id: "system-status",
                bounds: Rect { x: 16, y: 160, width: 200, height: 48 },
                kind: CompiledNodeKind::StatusIndicator(StatusIndicatorSpec {
                    requirement_id: "REQ-TEST-001",
                    source: "MONITOR_STATUS",
                    state_text_keys: &["STR-NS-NOMINAL", "STR-NS-ALERT"],
                    color_tokens: &["Theme.Colors.A", "Theme.Colors.B"],
                }),
            },
        ],
        golden_references: &[],
    };

    fn bindings() -> ScreenBindings {
        ScreenBindings::from_screen(
            &TEST_SCREEN,
            default_standard_text_package().expect("standard package"),
            default_display_text_packages().expect("display packages"),
            &[],
            "en-US",
        )
        .expect("bindings resolve")
    }

    #[test]
    fn consecutive_events_batch_into_one_frame_and_later_checks_reuse_it() {
        let bindings = bindings();
        let mut frame_inputs = FrameInputs::from_bindings(&bindings).expect("frame inputs");
        let mut events = FrameEvents::new();

        let mut patient_id = TextInputModel::new("PATIENT_ID", 16);
        let realtime_calls = std::rc::Rc::new(std::cell::Cell::new(0u32));
        let realtime_calls_for_closure = std::rc::Rc::clone(&realtime_calls);

        let mut input = move |events: &mut FrameEvents, frame: &mut FrameInputs| {
            for event in events.drain() {
                patient_id.apply(&event);
            }
            frame.set_text("PATIENT_ID", patient_id.as_str()).expect("PATIENT_ID wiring");
        };
        let mut realtime = move |frame: &mut FrameInputs| {
            realtime_calls_for_closure.set(realtime_calls_for_closure.get() + 1);
            frame.set_status("MONITOR_STATUS", 0).expect("MONITOR_STATUS wiring");
        };

        let script = ScenarioScript {
            id: "batching-test",
            requirement_ids: &[],
            clock: ScenarioClock { year: 2026, month: 1, day: 1, hours: 0, minutes: 0, seconds: 0 },
            steps: &[
                ScenarioStep::Event(WidgetEvent::CharTyped { source: "PATIENT_ID", character: 'A' }),
                ScenarioStep::Event(WidgetEvent::CharTyped { source: "PATIENT_ID", character: 'B' }),
                ScenarioStep::ExpectText { source: "PATIENT_ID", value: "AB" },
                ScenarioStep::ExpectStatus { source: "MONITOR_STATUS", value: 0 },
                ScenarioStep::Capture { label: "after-typing" },
            ],
        };

        let mut captured = Vec::new();
        let trace = run_scenario(
            &script,
            &mut events,
            &mut frame_inputs,
            &mut input,
            &mut realtime,
            |label, _frame_inputs| captured.push(label.to_string()),
        );

        assert!(trace.passed, "{trace:#?}");
        assert_eq!(trace.steps.len(), 5);
        assert_eq!(trace.capture_labels, vec!["after-typing"]);
        assert_eq!(captured, vec!["after-typing".to_string()]);
        // Two queued CharTyped events settle in one frame; the ExpectStatus and Capture steps
        // that follow read the same settled state without ticking realtime again.
        assert_eq!(realtime_calls.get(), 1);
    }

    #[test]
    fn a_failed_expectation_fails_the_trace_but_replay_continues() {
        let bindings = bindings();
        let mut frame_inputs = FrameInputs::from_bindings(&bindings).expect("frame inputs");
        let mut events = FrameEvents::new();

        let mut input = |events: &mut FrameEvents, frame: &mut FrameInputs| {
            for event in events.drain() {
                if let WidgetEvent::CharTyped { source, character } = event {
                    let mut current = frame.text(source).unwrap_or("").to_string();
                    current.push(character);
                    frame.set_text(source, &current).expect("PATIENT_ID wiring");
                }
            }
        };
        let mut realtime = |frame: &mut FrameInputs| {
            frame.set_status("MONITOR_STATUS", 1).expect("MONITOR_STATUS wiring");
        };

        let script = ScenarioScript {
            id: "failure-test",
            requirement_ids: &[],
            clock: ScenarioClock { year: 2026, month: 1, day: 1, hours: 0, minutes: 0, seconds: 0 },
            steps: &[
                ScenarioStep::Event(WidgetEvent::CharTyped { source: "PATIENT_ID", character: 'A' }),
                // Wrong on purpose: the model above only ever appends, so this never holds.
                ScenarioStep::ExpectText { source: "PATIENT_ID", value: "WRONG" },
                ScenarioStep::ExpectStatus { source: "MONITOR_STATUS", value: 1 },
            ],
        };

        let trace = run_scenario(
            &script,
            &mut events,
            &mut frame_inputs,
            &mut input,
            &mut realtime,
            |_label, _frame_inputs| {},
        );

        assert!(!trace.passed);
        assert_eq!(trace.steps.len(), 3);
        assert!(!trace.steps[1].passed);
        assert_eq!(trace.steps[1].observed.as_deref(), Some("A"));
        assert_eq!(trace.steps[1].expected.as_deref(), Some("WRONG"));
        // Replay continues past the failed expectation, and the still-true one still passes.
        assert!(trace.steps[2].passed);
    }

    #[test]
    fn advance_runs_exactly_n_settles_immediately() {
        let bindings = bindings();
        let mut frame_inputs = FrameInputs::from_bindings(&bindings).expect("frame inputs");
        let mut events = FrameEvents::new();

        let realtime_calls = std::rc::Rc::new(std::cell::Cell::new(0u32));
        let realtime_calls_for_closure = std::rc::Rc::clone(&realtime_calls);

        let mut input = |_events: &mut FrameEvents, _frame: &mut FrameInputs| {};
        let mut realtime = move |frame: &mut FrameInputs| {
            let tick = realtime_calls_for_closure.get() + 1;
            realtime_calls_for_closure.set(tick);
            // Only the 5th (final) settle should flip status to ALERT — proves each Advance
            // iteration genuinely re-invokes the realtime closure rather than settling once.
            frame
                .set_status("MONITOR_STATUS", if tick >= 5 { 1 } else { 0 })
                .expect("MONITOR_STATUS wiring");
        };

        let script = ScenarioScript {
            id: "advance-test",
            requirement_ids: &[],
            clock: ScenarioClock { year: 2026, month: 1, day: 1, hours: 0, minutes: 0, seconds: 0 },
            steps: &[
                ScenarioStep::Advance { frames: 5 },
                ScenarioStep::ExpectStatus { source: "MONITOR_STATUS", value: 1 },
            ],
        };

        let trace = run_scenario(
            &script,
            &mut events,
            &mut frame_inputs,
            &mut input,
            &mut realtime,
            |_label, _frame_inputs| {},
        );

        // Five explicit settles from Advance, then the ExpectStatus step must not trigger a
        // sixth (state is already settled).
        assert_eq!(realtime_calls.get(), 5, "{trace:#?}");
        assert_eq!(trace.steps[0].description, "advance frames=5");
        assert!(trace.steps[0].passed);
    }

    #[test]
    fn a_dropped_event_fails_its_step_instead_of_silently_passing() {
        let bindings = bindings();
        let mut frame_inputs = FrameInputs::from_bindings(&bindings).expect("frame inputs");
        // Capacity 1: the second queued event has nowhere to go before the frame settles.
        let mut events = FrameEvents::with_capacity(1);

        // Neither closure runs: with no Expect*/Capture step in the script, settle_frame is
        // never called, so both Event steps push directly into the still-full queue.
        let mut input = |_events: &mut FrameEvents, _frame: &mut FrameInputs| {};
        let mut realtime = |_frame: &mut FrameInputs| {};

        let script = ScenarioScript {
            id: "overflow-test",
            requirement_ids: &[],
            clock: ScenarioClock { year: 2026, month: 1, day: 1, hours: 0, minutes: 0, seconds: 0 },
            steps: &[
                ScenarioStep::Event(WidgetEvent::CharTyped { source: "PATIENT_ID", character: 'A' }),
                ScenarioStep::Event(WidgetEvent::CharTyped { source: "PATIENT_ID", character: 'B' }),
            ],
        };

        let trace = run_scenario(
            &script,
            &mut events,
            &mut frame_inputs,
            &mut input,
            &mut realtime,
            |_label, _frame_inputs| {},
        );

        assert!(!trace.passed, "a dropped event must fail the scenario, not pass silently");
        assert!(trace.steps[0].passed, "the first event fit in the queue and should pass");
        assert!(!trace.steps[1].passed, "the second event was dropped and must fail");
        assert_eq!(
            trace.steps[1].observed.as_deref(),
            Some("dropped: FrameEvents queue is full")
        );
    }
}
