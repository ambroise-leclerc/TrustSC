//! The application's own interaction and realtime state (ADR-015/ADR-013), factored out of
//! `main` so both the windowed binary and the scenario test (`tests/scenarios.rs`) register the
//! exact same closures with the exact same starting state — there is no second, test-only
//! implementation of the monitor's behavior to drift from what actually ships.

use std::{cell::Cell, rc::Rc};

use mdux::realtime::FrameInputs;
use mdux::{FrameEvents, TextInputModel, WidgetEvent};

/// Synthetic EEG: two drifting spectral peaks over pseudo-noise; the sedation index follows the
/// dominant peak. Stands in for the acquisition front-end a real device would have.
pub struct EegSimulator {
    tick: u32,
    noise: u32,
}

impl EegSimulator {
    pub fn new() -> Self {
        Self { tick: 0, noise: 0x9E37_79B9 }
    }

    pub fn tick(&mut self) -> (i64, [f32; 64]) {
        self.tick += 1;
        let time = self.tick as f32 / 60.0;
        let peak_a = 12.0 + 6.0 * (time / 5.0).sin();
        let peak_b = 38.0 + 10.0 * (time / 9.0).cos();
        let mut row = [0.0f32; 64];
        for (bin, sample) in row.iter_mut().enumerate() {
            self.noise = self.noise.wrapping_mul(1_664_525).wrapping_add(1_013_904_223);
            let noise = (self.noise >> 24) as f32 / 255.0 * 0.12;
            let lobe = |peak: f32, width: f32| (-((bin as f32 - peak) / width).powi(2)).exp();
            *sample = (0.85 * lobe(peak_a, 4.0) + 0.55 * lobe(peak_b, 7.0) + noise).min(1.0);
        }
        let index = (46.0 + 18.0 * (time / 7.0).sin()).clamp(0.0, 99.0) as i64;
        (index, row)
    }

    /// The synthetic alert fires every ~20 s at the nominal 60 Hz tick rate.
    fn is_alert_tick(&self) -> bool {
        self.tick % 1200 == 0
    }
}

impl Default for EegSimulator {
    fn default() -> Self {
        Self::new()
    }
}

/// Owns the application state the registered closures capture: the patient-identifier buffer
/// (ADR-015 controlled component, REQ-NS-005), the shared alert flag the ACK button clears
/// (REQ-NS-004), and the EEG simulator feeding the realtime bindings.
pub struct AppLogic {
    patient_id: TextInputModel,
    alert_active: Rc<Cell<bool>>,
    simulator: EegSimulator,
}

impl AppLogic {
    pub fn new() -> Self {
        Self {
            patient_id: TextInputModel::new("PATIENT_ID", 16),
            alert_active: Rc::new(Cell::new(false)),
            simulator: EegSimulator::new(),
        }
    }

    /// Builds the two closures `main` registers via `mdux_vulkan_winit::App::with_input` /
    /// `with_realtime` — and the scenario test registers with `mdux::verify_scenario::run_scenario`
    /// instead. Consumes `self`: both closures move their share of its state, so callers construct
    /// a fresh `AppLogic` per run (per windowed session, or per scenario in the test).
    pub fn into_closures(
        self,
    ) -> (impl FnMut(&mut FrameEvents, &mut FrameInputs), impl FnMut(&mut FrameInputs)) {
        let AppLogic {
            mut patient_id,
            alert_active,
            mut simulator,
        } = self;
        let alert_for_input = Rc::clone(&alert_active);

        let input = move |events: &mut FrameEvents, frame: &mut FrameInputs| {
            for event in events.drain() {
                match event {
                    WidgetEvent::ButtonPressed { source: "ACK_BUTTON" } => {
                        alert_for_input.set(false);
                    }
                    other => {
                        patient_id.apply(&other);
                    }
                }
            }
            frame.set_text("PATIENT_ID", patient_id.as_str()).expect("PATIENT_ID wiring");
        };

        let realtime = move |frame: &mut FrameInputs| {
            let (index, row) = simulator.tick();
            // A synthetic alert fires every ~20 s at the nominal 60 Hz and latches until the
            // operator acknowledges it.
            if simulator.is_alert_tick() {
                alert_active.set(true);
            }
            let status = if alert_active.get() { 1 } else { 0 };
            frame.set_number("SEDATION_INDEX", index).expect("SEDATION_INDEX wiring");
            frame.set_status("MONITOR_STATUS", status).expect("MONITOR_STATUS wiring");
            frame.push_row("EEG_DSA", &row).expect("EEG_DSA wiring");
        };

        (input, realtime)
    }
}

impl Default for AppLogic {
    fn default() -> Self {
        Self::new()
    }
}
