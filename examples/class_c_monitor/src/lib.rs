//! Library surface backing the `class_c_monitor` binary, so `tests/scenarios.rs` can drive the
//! exact same interaction logic `main` registers with `trustsc_vulkan_winit::App` — no GPU, no
//! window, just the ADR-015 event plane replayed by `trustsc::verify_scenario::run_scenario`.
//!
//! The baked EEG depth-of-anesthesia model (ADR-017) is brought into scope here so `app_logic`
//! can build its classifier from it; see `build.rs` for where the committed
//! `generated/models/eeg-demo/package.json` is compiled into this module.

trustsc::include_model!();

pub mod app_logic;
