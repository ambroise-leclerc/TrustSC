# Getting started

This page walks through two complete example applications end to end — the smallest possible
TrustSC app and a fuller Class C monitor — plus the Vulkan prerequisites and full command
reference. For the high-level pitch and a condensed quickstart, see the
[README](../README.en.md). For how the pieces fit together architecturally, see
[Architecture](architecture.md).

## A complete medical UI app in ~60 lines

This is the entire `examples/hello_world` application — a demo that models a Class B device per
IEC 62304 (a requirement, a verification case, an audit trail, and a Vulkan-rendered screen) using
this framework's compliance APIs, not a certified medical device. No `ash`, `winit`, or `shaderc`
dependency of its own.

`hello_world.medui`:

```
Screen HelloWorld {
    layout: Vertical { spacing: 16px; padding: 24px; }

    @safety_critical(cv_check: [Bounds, ColorHash])
    CriticalButton {
        id: hello-world-label;
        requirement: "REQ-HELLO-001";
        width: Fill;
        height: 80px;
        label: t("STR-HELLO-WORLD");
        color: Theme.Colors.PrimaryAction;
        on_press: SystemEvent.NoOp;
    }

    VulkanViewport {
        id: hello-world-viewport;
        width: Fill;
        height: 280px;
        stream_source: "HELLO_WORLD_SIM";
    }
}
```

`build.rs`:

```rust
fn main() -> Result<(), Box<dyn std::error::Error>> {
    trustsc_build::MeduiScreen::new("hello_world.medui")
        .surface(800, 480)
        .compile()
}
```

`src/main.rs`:

```rust
trustsc::include_medui_screen!();

use trustsc::{
    ComplianceProgram, DeviceContext, FrameworkBuilder, Requirement, RequirementId, SafetyClass,
    UiSdkConfig, VerificationCase, VerificationMethod,
};

fn main() -> Result<(), Box<dyn std::error::Error>> {
    let device = DeviceContext::new(
        "Acme Medical",
        "TrustSC Hello World",
        "hello-world-ui",
        "0.1.0",
        SafetyClass::B,
    )?;
    let requirement_id = RequirementId::new("REQ-HELLO-001")?;

    let mut compliance = ComplianceProgram::new(device.clone());
    compliance.add_requirement(Requirement::new(
        requirement_id.clone(),
        "Render the hello-world greeting",
        "IEC62304-5.2",
        "Verify the smoke demo renders a greeting component",
    )?);
    compliance.add_verification(VerificationCase::new(
        "VER-HELLO-001",
        requirement_id,
        VerificationMethod::Test,
        "Preview frame execution in the host smoke demo",
    )?);

    let framework = FrameworkBuilder::new()
        .with_device(device)
        .with_compliance(compliance)
        .with_ui(UiSdkConfig::vulkan_class_b(800, 480, 16))
        .with_screen(medui_screen::screen())
        .build()?;

    trustsc_vulkan_winit::App::new(framework, medui_screen::screen()).run_from_env()
}
```

`Cargo.toml` needs only `trustsc` + `trustsc-vulkan-winit` as dependencies and `trustsc-build` as a
build-dependency — see `examples/hello_world/Cargo.toml`. Run it with `cargo run -p hello_world`
(opens a window), `-- --auto-close-ms=1000` (closes itself, useful for manual smoke checks), or
`-- --headless-smoke` (no window, no Vulkan at all — for CI).

Everything generic — the Vulkan instance/device/swapchain/pipeline, the winit event loop, the
glyph-atlas upload, and the CLI flags — lives in `adapters/trustsc-vulkan-winit`, reused by every
application; see [Hello World Vulkan text path](#hello-world-vulkan-text-path) below.

## A complete Class C monitor: 3D UI + zero-SOUP ML in ~400 lines

`examples/class_c_monitor` is the **Acme NeuroSense 500**, a fictional depth-of-anesthesia
monitor modeling a genuine IEC 62304 **Class C** configuration. It combines, in one screen: a
**3D spectral waterfall** (`VulkanViewport`) rendering the live EEG spectrogram; ADR-014's
**pixel-exact positioned layout** (a 1920×1080 surface pinned in the `.medui` file itself, a
512×512 sedation-index box with 160 px = 120 pt digits); ADR-015's **operator interaction** (a
bounded patient-identifier `TextInput` and an ACKNOWLEDGE `Button`); and, since ADR-017/018, a
**real, on-device machine-learning classifier** — no ONNX Runtime, no PyTorch, just
`Classifier1D` running the same zero-allocation, `#![forbid(unsafe_code)]` inference engine a
Phase 2 clinical build would ship unchanged. Every `position:` is verified by the compiler
(containment, no-overlap, i18n text budgets inside the pinned box) and pinned as an automatic
golden reference — the layout specification *is* the evidence:

```text
+---------------------------------------------------------------------------+
| NeuroSense 500 - Depth of...  2026-07-04 14:25:09    NOMINAL   [A= logo]  |  topbar 1920x64
+---------------------------------------------------+-----------------------+
|                                                   |       512x512        |
|                                                   |      [  5 0  ]       |
|          EEG DSA waterfall 1360x984               |    160px digits      |
|                                                   +-----------------------+
|                                                   | PATIENT ID           |
|                                                   | [PID-2026 47_     ]  |  TextInput (1392,640) 512x48
|                                                   | [ ACKNOWLEDGE ]      |  Button    (1392,720) 240x64
|                                                   | RAW EEG              |
|                                                   | [~/\/\/\/\/\/\~]     |  SignalTrace (1392,824) 512x224
+---------------------------------------------------+-----------------------+
```

If a future translation outgrows the pinned title box, or two positioned components collide, or
the 16-character identifier budget no longer fits its box, **the build fails** — the alert
happens at compile time, never on a bench. Interaction flows one way out through the bounded
`FrameEvents` queue and one way back in through `set_text` (the application owns the buffer; the
renderer stores nothing), the clock still costs zero application code, and the app still has no
`ash`/`winit`/`shaderc` dependency.

**`neurosense.medui`** (103 lines) — the `SignalTrace` node at the bottom reserves the raw-EEG
strip; everything else is exactly the positioned layout ADR-014 already established:

```text
Screen NeuroSense500 {
    layout: Vertical { spacing: 8px; padding: 0px; }
    surface: 1920px, 1080px;
    Row {
        id: topbar;
        height: 64px;
        background: Theme.Colors.TopbarBackground;
        Label { id: device-title; width: 340px; height: 48px; position: 16px, 8px; text: t("STR-NS-TITLE"); color: Theme.Colors.Title; }
        Clock { id: wall-clock; width: 448px; height: 48px; position: 372px, 8px; format: DateTimeSeconds; }
        StatusIndicator {
            id: system-status;
            width: 200px;
            height: 48px;
            position: 1552px, 8px;
            requirement: "REQ-NS-003";
            source: "MONITOR_STATUS";
            states: [t("STR-NS-NOMINAL"), t("STR-NS-ALERT"), t("STR-NS-FAULT")];
        }
        Image { id: acme-logo; width: 144px; height: 48px; position: 1768px, 8px; source: img("LOGO-ACME"); }
    }
    @safety_critical(cv_check: [Bounds, ColorHash])
    NumericDisplay {
        id: sedation-index;
        width: 512px;
        height: 512px;
        position: 1392px, 80px;
        requirement: "REQ-NS-001";
        template: "TPL-SEDATION-INDEX-160";
        source: "SEDATION_INDEX";
        color: Theme.Colors.ScoreDigits;
    }
    Label { id: patient-id-caption; width: 200px; height: 24px; position: 1392px, 608px; text: t("STR-NS-PATIENT-ID"); color: Theme.Colors.Title; }
    TextInput {
        id: patient-id-input;
        width: 512px;
        height: 48px;
        position: 1392px, 640px;
        requirement: "REQ-NS-005";
        source: "PATIENT_ID";
        max_length: 16;
        charset: AsciiText;
        color: Theme.Colors.Title;
    }
    Button {
        id: ack-button;
        width: 240px;
        height: 64px;
        position: 1392px, 720px;
        requirement: "REQ-NS-004";
        label: t("STR-NS-ACK");
        color: Theme.Colors.PrimaryAction;
        source: "ACK_BUTTON";
    }
    Label { id: eeg-trace-caption; width: 300px; height: 20px; position: 1392px, 800px; text: t("STR-NS-EEG-TRACE-CAPTION"); color: Theme.Colors.Title; }
    SignalTrace {
        id: eeg-trace;
        width: 512px;
        height: 224px;
        position: 1392px, 824px;
        stream_source: "EEG_TRACE";
        color: Theme.Colors.Nominal;
    }
    VulkanViewport { id: eeg-dsa; width: 1360px; height: 984px; position: 16px, 80px; stream_source: "EEG_DSA"; }
}
```

**`build.rs`** (10 lines) — one extra line over the pre-ML version, `ModelPackage::new(..)`, is
the entire cost of embedding the classifier:

```rust
fn main() -> Result<(), Box<dyn std::error::Error>> {
    trustsc_build::MeduiScreen::new("neurosense.medui")
        .surface(1920, 1080)
        .compile()?;
    // Phase 1 (Hugging Face-style demonstrator) points this at generated/models/eeg-demo/package.json;
    // Phase 2 (production) repoints it at a manufacturer's own clinically-qualified weights baked
    // by the same tools/trustsc-ml-baker pipeline — zero change below this line (ADR-017 §2).
    trustsc_build::ModelPackage::new("../../generated/models/eeg-demo/package.json").compile()?;
    trustsc_build::ScenarioSet::new("verify/scenarios").compile()
}
```

**`src/app_logic.rs`** (188 lines total; the realtime closure below is the flagship
demonstration of the "weights are data" story) — `MODEL` is whatever
`generated/models/eeg-demo/package.json` the build compiled in. Swap that one committed file for
a manufacturer's own clinically-qualified weights, baked by the exact same `tools/trustsc-ml-baker`
pipeline, and every line of application code stays unchanged:

```rust
static MODEL: LazyLock<trustsc::ModelPackage> = LazyLock::new(crate::medui_model::model);

// ... inside AppLogic::into_closures(), built once (not per frame):
let classifier = Classifier1D::<CLASSIFIER_MAX_UNITS, CLASSIFIER_MAX_OUT>::new(&MODEL)
    .expect("baked eeg-demo model should pass its golden self-test (ADR-017 §4)");

let realtime = move |frame: &mut FrameInputs| {
    let (row, raw) = simulator.tick();

    // Zero-allocation inference over the current spectral row (ADR-017): pure arithmetic, no
    // SOUP, the same engine a Phase 2 production build would ship unchanged.
    let scaled: [f32; 64] = std::array::from_fn(|i| row[i] * ENERGY_SCALE);
    let prediction = classifier.predict(&scaled).expect("row always matches input_spec");
    let scores = prediction.scores();

    // Sedation index blends the class probabilities into a single 0-99 score: near 100 fully
    // awake, mid-range adequately anesthetized, near 0 burst-suppressed. get() rather than
    // indexing so a differently-shaped committed model can't panic here.
    let awake = scores.get(usize::from(CLASS_AWAKE)).copied().unwrap_or(0.0);
    let adequate = scores.get(CLASS_ADEQUATE).copied().unwrap_or(0.0);
    let index = (awake * 99.0 + adequate * 50.0).round() as i64;

    // A detected burst-suppression state latches the alert until the operator acknowledges it
    // (REQ-NS-004, HAZ-NS-002) -- the classifier decides it is alarming, not a fake timer.
    if prediction.class == CLASS_BURST_SUPPRESSION {
        alert_active.set(true);
    }
    let status = if alert_active.get() { 2 } else if prediction.class == CLASS_AWAKE { 1 } else { 0 };

    frame.set_number("SEDATION_INDEX", index.clamp(0, 99)).expect("SEDATION_INDEX wiring");
    frame.set_status("MONITOR_STATUS", status).expect("MONITOR_STATUS wiring");
    frame.push_row("EEG_DSA", &row).expect("EEG_DSA wiring");
    frame.push_sample("EEG_TRACE", raw).expect("EEG_TRACE wiring");
};
```

`src/main.rs` (92 lines) wires the `DeviceContext`/`ComplianceProgram`/`UiSdkConfig` and
registers the closures above with `trustsc_vulkan_winit::App` — the same boilerplate every
`FrameworkBuilder`-based example needs, unrelated to the ML story; see the file directly if
you're after the compliance plumbing rather than the classifier.

Run it with `cargo run -p class_c_monitor` (windowed; type into the patient-ID field — click or
Tab to focus, arrows/Home/End move the caret — and acknowledge the alert that fires roughly every
40 s (first episode ~20 s after startup) as a scheduled burst-suppression episode drives the real
classifier's output; note the `HOST PREVIEW` banner, the scrolling green `RAW EEG` trace
flattening during the episode exactly like real isoelectric EEG, and the `runtime` audit event in
the diagnostics), or `-- --headless-smoke` for the CI path — the smoke output shows
`golden_refs=11`: every positioned node is pinned evidence.

## Vulkan prerequisites

The primary development path for TrustSC is Vulkan-based medical UI work. Install a system Vulkan
loader before running the windowed examples.

### macOS

```bash
brew install vulkan-loader molten-vk vulkan-tools
export VK_ICD_FILENAMES="$(brew --prefix)/etc/vulkan/icd.d/MoltenVK_icd.json"
export DYLD_FALLBACK_LIBRARY_PATH="$(brew --prefix)/lib${DYLD_FALLBACK_LIBRARY_PATH:+:$DYLD_FALLBACK_LIBRARY_PATH}"
vulkaninfo | head
```

`vulkan-loader` provides `libvulkan.dylib`, `molten-vk` supplies the Vulkan-on-Metal driver, and
`vulkan-tools` provides `vulkaninfo`. The extra `DYLD_FALLBACK_LIBRARY_PATH` export makes
Cargo-launched binaries find Homebrew's `libvulkan.dylib` on macOS.

To make those variables permanent in the default macOS shell:

```bash
cat <<'EOF' >> ~/.zshrc
export VK_ICD_FILENAMES="$(brew --prefix)/etc/vulkan/icd.d/MoltenVK_icd.json"
export DYLD_FALLBACK_LIBRARY_PATH="$(brew --prefix)/lib${DYLD_FALLBACK_LIBRARY_PATH:+:$DYLD_FALLBACK_LIBRARY_PATH}"
EOF
source ~/.zshrc
```

### Ubuntu / Debian

```bash
sudo apt-get update
sudo apt-get install libvulkan1 libvulkan-dev vulkan-tools
vulkaninfo | head
```

If you only need non-graphical validation, `cargo run -p hello_world -- --headless-smoke` still
works without the windowed Vulkan path.

## Commands

```bash
source $HOME/.cargo/env
cd TrustSC

# build everything
cargo build

# run all tests
cargo test

# run a single test
cargo test builds_framework_from_screen_through_public_api

# run the shortest demo (opens a Vulkan window; requires a system Vulkan loader such as libvulkan.dylib / MoltenVK)
cargo run -p hello_world

# run it and close automatically after one second
cargo run -p hello_world -- --auto-close-ms=1000

# run the same smoke path without a window
cargo run -p hello_world -- --headless-smoke

# run the Class C realtime monitor: 3D waterfall + UI + zero-SOUP ML (windowed, ADR-013 host preview)
cargo run -p class_c_monitor
cargo run -p class_c_monitor -- --headless-smoke

# run the richer examples
cargo run -p class_b_device
cargo run -p class_c_vulkansc_device

# inspect the text-asset pipeline tooling
cargo run -p trustsc-text-authoring --bin trustsc-textc -- describe-pipeline

# render a screen offscreen and check every golden reference against the rendered pixels
# (ADR-016 — full operator guide: docs/verification/ui-verification.md)
cargo run -p hello_world -- --verify-ui=generated/verification --locales=en-US
cargo run -p class_c_monitor -- --verify-ui=generated/verification --locales=all
```

The default `hello_world` example opens a real Vulkan window and requires a system Vulkan loader.
Install the Vulkan prerequisites above, or use `--headless-smoke` when validating the framework in
a non-graphical environment.

The same example also includes a minimal `.medui` source file compiled at build time into a
static screen package. The generated package drives the hello-world text key, the text origin
used by the Vulkan overlay, the emitted golden-reference entries for the safety-critical button,
and compile-time rejection when an approved translation would overflow the allocated UI bounds.

## Hello World Vulkan text path

- `examples/hello_world/hello_world.medui` is the entire application-specific content;
  `examples/hello_world/build.rs` compiles it via `trustsc-build`'s `MeduiScreen` into generated
  screen metadata, brought into scope with `trustsc::include_medui_screen!()`.
- `trustsc::screen_text::ScreenTextLayout` (in `crates/trustsc`) resolves the screen's approved text
  into glyph draw commands — this is generic, screen-agnostic logic reused by every application.
- `adapters/trustsc-vulkan-winit` owns everything platform-specific: it uploads the glyph atlas and
  renders textured quads using shaders precompiled to SPIR-V and committed under
  `adapters/trustsc-vulkan-winit/shaders/generated/` (see `tools/trustsc-shader-baker`), so
  applications need no `ash`/`winit`/`shaderc` dependency of their own.
- Use `cargo run -p hello_world -- --auto-close-ms=1000` to smoke-test the actual Vulkan text
  overlay path when a system Vulkan loader is available.
- `cargo run -p hello_world -- --headless-smoke` is still useful for non-graphical environments,
  but it intentionally skips the windowed Vulkan text-rendering path.
