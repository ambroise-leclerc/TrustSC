//! Library surface backing the `class_c_monitor` binary, so `tests/scenarios.rs` can drive the
//! exact same interaction logic `main` registers with `mdux_vulkan_winit::App` — no GPU, no
//! window, just the ADR-015 event plane replayed by `mdux::verify_scenario::run_scenario`.

pub mod app_logic;
