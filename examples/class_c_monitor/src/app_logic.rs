//! The application's own interaction and realtime state (ADR-015/ADR-013/ADR-017), factored out
//! of `main` so both the windowed binary and the scenario test (`tests/scenarios.rs`) register
//! the exact same closures with the exact same starting state — there is no second, test-only
//! implementation of the monitor's behavior to drift from what actually ships.
//!
//! This is the flagship demonstration of the "weights are data" story (ADR-017 §2): `MODEL` is
//! whatever `generated/models/eeg-demo/package.json` the build compiled in. Swap that one
//! committed file for a manufacturer's own clinically-qualified weights — baked by the exact same
//! `tools/mdux-ml-baker` pipeline — and every line below is unchanged.

use std::sync::LazyLock;
use std::{cell::Cell, rc::Rc};

use mdux::realtime::FrameInputs;
use mdux::{Classifier1D, FrameEvents, TextInputModel, WidgetEvent};

/// Sized from the baked `eeg-demo` model's `max_layer_units()` (its widest activation is the
/// flattened 64-bin input) with headroom; `MAX_OUT` covers the three labels (AWAKE, ADEQUATE,
/// BURST_SUPPRESSION) with headroom. A different committed model that needs more must widen
/// these constants — `Classifier1D::new` fails fast with a clear error if they are too small.
const CLASSIFIER_MAX_UNITS: usize = 128;
const CLASSIFIER_MAX_OUT: usize = 4;

/// `AWAKE`/`ADEQUATE`/`BURST_SUPPRESSION` class indices, matching
/// `tools/mdux-ml-baker/fixtures/eeg-demo.toml`'s `output.labels` order exactly.
const CLASS_AWAKE: u8 = 0;
const CLASS_ADEQUATE: usize = 1;
const CLASS_BURST_SUPPRESSION: u8 = 2;

/// Brings the simulator's spectral row (baseline total energy ≈17, see `EegSimulator::tick`)
/// into the baked model's threshold bands (BURST_SUPPRESSION < 30, ADEQUATE 30-50, AWAKE > 50 —
/// see the fixture's own comments), without altering the row values the 3D waterfall (`EEG_DSA`)
/// renders.
const ENERGY_SCALE: f32 = 2.4;

/// The committed model package, loaded once for the process (ADR-013: no per-frame allocation
/// or object construction — this `LazyLock` initializes on first use, well before the render
/// loop starts, not per frame).
static MODEL: LazyLock<mdux::ModelPackage> = LazyLock::new(crate::medui_model::model);

/// Synthetic EEG: two drifting spectral peaks over pseudo-noise feed both the 3D waterfall
/// (`EEG_DSA`) and, scaled by [`ENERGY_SCALE`] into the baked model's threshold bands, the real
/// `Classifier1D` driving the sedation index and alert. A scheduled window periodically
/// attenuates the simulated signal toward near-isoelectric — a stand-in for a transient
/// burst-suppression episode a real device would have to detect, not a fake alert override: the
/// classifier is what actually decides it is alarming.
pub struct EegSimulator {
    tick: u32,
    noise: u32,
}

impl EegSimulator {
    pub fn new() -> Self {
        Self { tick: 0, noise: 0x9E37_79B9 }
    }

    /// The scheduled burst-suppression episode recurs every ~40s at the nominal 60Hz tick rate
    /// (first episode starting ~20s after startup), sustained for ~2.5s so a windowed run and
    /// scenario replay both see it clearly.
    fn in_suppression_episode(tick: u32) -> bool {
        (1200..1350).contains(&(tick % 2400))
    }

    /// One frame's raw spectral row (`EEG_DSA`) and a same-tick raw time-domain sample
    /// (`EEG_TRACE`), derived from the row's own average energy as an envelope so the raw trace
    /// naturally flattens during a burst-suppression episode — exactly like real isoelectric EEG.
    pub fn tick(&mut self) -> ([f32; 64], f32) {
        self.tick += 1;
        let time = self.tick as f32 / 60.0;
        let peak_a = 12.0 + 6.0 * (time / 5.0).sin();
        let peak_b = 38.0 + 10.0 * (time / 9.0).cos();
        let attenuation = if Self::in_suppression_episode(self.tick) { 0.05 } else { 1.0 };
        let mut row = [0.0f32; 64];
        for (bin, sample) in row.iter_mut().enumerate() {
            self.noise = self.noise.wrapping_mul(1_664_525).wrapping_add(1_013_904_223);
            let noise = (self.noise >> 24) as f32 / 255.0 * 0.12;
            let lobe = |peak: f32, width: f32| (-((bin as f32 - peak) / width).powi(2)).exp();
            *sample =
                ((0.85 * lobe(peak_a, 4.0) + 0.55 * lobe(peak_b, 7.0) + noise) * attenuation)
                    .min(1.0);
        }
        let envelope = row.iter().sum::<f32>() / row.len() as f32;
        let raw = envelope * (time * 2.0 * std::f32::consts::PI * 8.0).sin();
        (row, raw)
    }
}

impl Default for EegSimulator {
    fn default() -> Self {
        Self::new()
    }
}

/// Owns the application state the registered closures capture: the patient-identifier buffer
/// (ADR-015 controlled component, REQ-NS-005), the shared alert flag the ACK button clears
/// (REQ-NS-004), and the EEG simulator feeding the realtime bindings and the classifier.
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
        let classifier = Classifier1D::<CLASSIFIER_MAX_UNITS, CLASSIFIER_MAX_OUT>::new(&MODEL)
            .expect("baked eeg-demo model should pass its golden self-test (ADR-017 \u{a7}4)");

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
            let (row, raw) = simulator.tick();

            // Zero-allocation inference over the current spectral row (ADR-017): pure
            // arithmetic, no SOUP, the same engine a Phase 2 production build would ship
            // unchanged. The row is scaled (not replaced) before classification so the waterfall
            // (`EEG_DSA`) keeps rendering the simulator's own, unscaled intensity values.
            let scaled: [f32; 64] = std::array::from_fn(|i| row[i] * ENERGY_SCALE);
            let prediction = classifier
                .predict(&scaled)
                .expect("fixed-size row always matches the model's input_spec");
            let scores = prediction.scores();

            // Sedation index blends the class probabilities into a single 0-99 score, the same
            // way a real depth-of-anesthesia index would: near 100 fully awake, mid-range
            // adequately anesthetized, near 0 burst-suppressed. Uses get().unwrap_or(0.0) rather
            // than indexing: a differently-shaped committed model (fewer than 2 classes) must
            // fall back to 0 contribution instead of panicking.
            let awake_score = scores.get(usize::from(CLASS_AWAKE)).copied().unwrap_or(0.0);
            let adequate_score = scores.get(CLASS_ADEQUATE).copied().unwrap_or(0.0);
            let index = (awake_score * 99.0 + adequate_score * 50.0).round() as i64;

            // A detected burst-suppression state latches the alert until the operator
            // acknowledges it (REQ-NS-004, HAZ-NS-002); a detected AWAKE state is shown live but
            // does not latch — it is not, on its own, the hazard this device is designed to
            // catch.
            if prediction.class == CLASS_BURST_SUPPRESSION {
                alert_active.set(true);
            }
            let status = if alert_active.get() {
                2 // FAULT: latched burst-suppression alert
            } else if prediction.class == CLASS_AWAKE {
                1 // ALERT: live AWAKE reading
            } else {
                0 // NOMINAL
            };

            frame.set_number("SEDATION_INDEX", index.clamp(0, 99)).expect("SEDATION_INDEX wiring");
            frame.set_status("MONITOR_STATUS", status).expect("MONITOR_STATUS wiring");
            frame.push_row("EEG_DSA", &row).expect("EEG_DSA wiring");
            frame.push_sample("EEG_TRACE", raw).expect("EEG_TRACE wiring");
        };

        (input, realtime)
    }
}

impl Default for AppLogic {
    fn default() -> Self {
        Self::new()
    }
}
