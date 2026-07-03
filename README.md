# MduX-rust

Medical-device manufacturer framework with Class B/Class C compliance modeling and a Vulkan / Vulkan SC-oriented UI SDK.

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
    mdux_build::MeduiScreen::new("hello_world.medui")
        .surface(800, 480)
        .compile()
}
```

`src/main.rs`:

```rust
mdux::include_medui_screen!();

use mdux::{
    ComplianceProgram, DeviceContext, FrameworkBuilder, Requirement, RequirementId, SafetyClass,
    UiSdkConfig, VerificationCase, VerificationMethod,
};

fn main() -> Result<(), Box<dyn std::error::Error>> {
    let device = DeviceContext::new(
        "Acme Medical",
        "MduX-rust Hello World",
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

    mdux_vulkan_winit::App::new(framework, medui_screen::screen()).run_from_env()
}
```

`Cargo.toml` needs only `mdux` + `mdux-vulkan-winit` as dependencies and `mdux-build` as a
build-dependency — see `examples/hello_world/Cargo.toml`. Run it with `cargo run -p hello_world`
(opens a window), `-- --auto-close-ms=1000` (closes itself, useful for manual smoke checks), or
`-- --headless-smoke` (no window, no Vulkan at all — for CI).

Everything generic — the Vulkan instance/device/swapchain/pipeline, the winit event loop, the
glyph-atlas upload, and the CLI flags — lives in `adapters/mdux-vulkan-winit`, reused by every
application; see [Hello World Vulkan text path](#hello-world-vulkan-text-path) below.

## A complete Class C monitor in 137 lines

`examples/class_c_monitor` is the **Acme NeuroSense 500**, a fictional depth-of-anesthesia
monitor modeling a genuine IEC 62304 **Class C** configuration: Vulkan SC profile with explicit
reserved budgets, a mandatory hazard, full requirement traceability — and a realtime bedside
layout, windowed on the development host through the ADR-013 preview:

```text
+----------------------------------------------------------------------+
| NeuroSense 500 - Depth of...   2026-07-03 14:25:09          NOMINAL  |  top bar
+----------------------------------------------------------------------+
|                                4 7                                   |  48px live index
+----------------------------------------------------------------------+
|            /\/\_/\  3D EEG spectral waterfall (DSA),                 |
|         __/       \__  one spectrum row per frame,                   |  VulkanViewport
|      __/     __       \____  history receding in perspective         |
+----------------------------------------------------------------------+
```

The clock costs **zero application code** (the adapter feeds platform time), every text budget
(including the wider French translations) is checked at compile time, and the app has no
`ash`/`winit`/`shaderc` dependency. The whole application:

**`neurosense.medui`** (45 lines):

```text
Screen NeuroSense500 {
    layout: Vertical { spacing: 8px; padding: 16px; }
    Row {
        id: topbar;
        height: 48px;
        spacing: 16px;
        Label {
            id: device-title;
            width: 340px;
            height: 48px;
            text: t("STR-NS-TITLE");
            color: Theme.Colors.Title;
        }
        Clock {
            id: wall-clock;
            width: Fill;
            height: 48px;
            format: DateTimeSeconds;
        }
        StatusIndicator {
            id: system-status;
            width: 200px;
            height: 48px;
            requirement: "REQ-NS-003";
            source: "MONITOR_STATUS";
            states: [t("STR-NS-NOMINAL"), t("STR-NS-ALERT"), t("STR-NS-FAULT")];
        }
    }
    @safety_critical(cv_check: [Bounds, ColorHash])
    NumericDisplay {
        id: sedation-index;
        width: Fill;
        height: 120px;
        requirement: "REQ-NS-001";
        template: "TPL-SEDATION-INDEX";
        source: "SEDATION_INDEX";
        color: Theme.Colors.ScoreDigits;
    }
    VulkanViewport {
        id: eeg-dsa;
        width: Fill;
        height: Fill;
        stream_source: "EEG_DSA";
    }
}
```

**`build.rs`** (5 lines):

```rust
fn main() -> Result<(), Box<dyn std::error::Error>> {
    mdux_build::MeduiScreen::new("neurosense.medui")
        .surface(1280, 720)
        .compile()
}
```

**`src/main.rs`** (87 lines, EEG simulator included):

```rust
mdux::include_medui_screen!();

use mdux::{
    ComplianceProgram, DeviceContext, FrameworkBuilder, Hazard, Requirement, RequirementId,
    SafetyClass, UiSdkConfig, VerificationCase, VerificationMethod,
};

/// Synthetic EEG: two drifting spectral peaks over pseudo-noise; the sedation index follows the
/// dominant peak. Stands in for the acquisition front-end a real device would have.
struct EegSimulator {
    tick: u32,
    noise: u32,
}

impl EegSimulator {
    fn tick(&mut self) -> (i64, [f32; 64]) {
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
}

fn main() -> Result<(), Box<dyn std::error::Error>> {
    let device = DeviceContext::new(
        "Acme Medical",
        "NeuroSense 500",
        "neurosense-ui",
        "0.1.0",
        SafetyClass::C,
    )?;

    let mut compliance = ComplianceProgram::new(device.clone());
    let req_index = RequirementId::new("REQ-NS-001")?;
    let req_stream = RequirementId::new("REQ-NS-002")?;
    let req_status = RequirementId::new("REQ-NS-003")?;
    for (id, verification_id, title) in [
        (&req_index, "VER-NS-001", "Display the sedation index, refreshed every frame"),
        (&req_stream, "VER-NS-002", "Display the spectral stream with visible freshness"),
        (&req_status, "VER-NS-003", "Keep the system status permanently visible"),
    ] {
        compliance.add_requirement(Requirement::new(
            id.clone(),
            title,
            "IEC62304-5.2",
            "Verified by windowed demonstration and headless smoke",
        )?);
        compliance.add_verification(VerificationCase::new(
            verification_id,
            id.clone(),
            VerificationMethod::Demonstration,
            "Windowed run on the development host",
        )?);
    }
    compliance.add_hazard(Hazard::new(
        "HAZ-NS-001",
        "A stale or frozen sedation index misleads the anesthesiologist",
        vec![req_index, req_stream],
    )?);

    let screen = medui_screen::screen();
    let framework = FrameworkBuilder::new()
        .with_device(device)
        .with_compliance(compliance)
        .with_ui(UiSdkConfig::vulkansc_class_c(1280, 720, 12, 32 * 1024 * 1024, 256))
        .with_screen(screen)
        .build()?;

    let mut simulator = EegSimulator { tick: 0, noise: 0x9E37_79B9 };
    mdux_vulkan_winit::App::new(framework, screen)
        .with_realtime(move |frame| {
            let (index, row) = simulator.tick();
            frame.set_number("SEDATION_INDEX", index).expect("SEDATION_INDEX wiring");
            frame.set_status("MONITOR_STATUS", 0).expect("MONITOR_STATUS wiring");
            frame.push_row("EEG_DSA", &row).expect("EEG_DSA wiring");
        })
        .run_from_env()
}
```

Run it with `cargo run -p class_c_monitor` (windowed; note the `HOST PREVIEW` banner and the
`runtime` audit event in the diagnostics), or `-- --headless-smoke` for the CI path.

## Vulkan prerequisites

The primary development path for MduX is Vulkan-based medical UI work. Install a system Vulkan loader before running the windowed examples.

### macOS

```bash
brew install vulkan-loader molten-vk vulkan-tools
export VK_ICD_FILENAMES="$(brew --prefix)/etc/vulkan/icd.d/MoltenVK_icd.json"
export DYLD_FALLBACK_LIBRARY_PATH="$(brew --prefix)/lib${DYLD_FALLBACK_LIBRARY_PATH:+:$DYLD_FALLBACK_LIBRARY_PATH}"
vulkaninfo | head
```

`vulkan-loader` provides `libvulkan.dylib`, `molten-vk` supplies the Vulkan-on-Metal driver, and `vulkan-tools` provides `vulkaninfo`. The extra `DYLD_FALLBACK_LIBRARY_PATH` export makes Cargo-launched binaries find Homebrew's `libvulkan.dylib` on macOS.

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

If you only need non-graphical validation, `cargo run -p hello_world -- --headless-smoke` still works without the windowed Vulkan path.

## Workspace layout

- `crates/mdux-core`: device metadata, safety classes, deterministic runtime policy
- `crates/mdux-governance`: requirements, hazards, verifications, audit trail, trace matrix export
- `crates/mdux-ui`: Vulkan / Vulkan SC UI policy and deterministic frame model
- `crates/mdux-ui-dsl-authoring`: host-side `.medui` compiler for generated static screen packages
- `crates/mdux-text-schema`: shared manifests and immutable compiled text-package schema
- `crates/mdux-text-authoring`: host-side font intake, deterministic atlas compilation, and asset tooling
- `crates/mdux-text-runtime`: no-allocation runtime text command generation from approved packages
- `crates/mdux`: thin facade for building complete framework instances, plus `screen_text` and
  `include_medui_screen!`
- `crates/mdux-build`: build-script helper (`MeduiScreen`) wrapping the `.medui` compiler
- `adapters/mdux-vulkan-winit`: the reusable Vulkan 1.0 + winit presentation adapter — the only
  crate depending on `ash`/`winit`
- `tools/mdux-font-baker`, `tools/mdux-shader-baker`: host-only bake/verify tools for the committed
  font atlas and SPIR-V shader evidence
- `examples/hello_world`: smallest out-of-the-box smoke demo (see above)
- `examples/class_b_device`: Class B Vulkan example
- `examples/class_c_monitor`: the NeuroSense 500 Class C realtime monitor (see above)
- `examples/class_c_vulkansc_device`: Class C Vulkan SC example (evidence-only, no window)

## Commands

```bash
source $HOME/.cargo/env
cd MduX-rust

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

# run the Class C realtime monitor (windowed, ADR-013 host preview)
cargo run -p class_c_monitor
cargo run -p class_c_monitor -- --headless-smoke

# run the richer examples
cargo run -p class_b_device
cargo run -p class_c_vulkansc_device

# inspect the text-asset pipeline tooling
cargo run -p mdux-text-authoring --bin mdux-textc -- describe-pipeline
```

The default `hello_world` example now opens a real Vulkan window and requires a system Vulkan loader. Install the Vulkan prerequisites above, or use `--headless-smoke` when validating the framework in a non-graphical environment.

The same example also includes a minimal `.medui` source file compiled at build time into a static screen package. The generated package now drives the hello-world text key, the text origin used by the Vulkan overlay, the emitted golden-reference entries for the safety-critical button, and compile-time rejection when an approved translation would overflow the allocated UI bounds.

## Continuous integration

- `.github/workflows/ci.yml` runs on `push`, `pull_request`, and manual dispatch so the checks execute on feature branches before merge.
- The workflow validates the Linux workspace with locked dependencies, runs the full test suite, verifies the committed Roboto (16 px and 48 px) and SPIR-V artifacts, and exercises `hello_world` and `class_c_monitor` through `--headless-smoke`.
- Replay the same checks locally with:

```bash
source $HOME/.cargo/env
cargo build --locked --workspace
cargo test --locked --quiet
cargo run --locked -q -p mdux-font-baker -- verify tools/mdux-font-baker/fixtures/roboto-demo.toml generated/fonts/roboto-regular-16px/package.json generated/fonts/roboto-regular-16px/report.json
cargo run --locked -q -p mdux-font-baker -- verify tools/mdux-font-baker/fixtures/roboto-display-48px.toml generated/fonts/roboto-display-48px/package.json generated/fonts/roboto-display-48px/report.json
cargo run --locked -q -p mdux-shader-baker -- verify tools/mdux-shader-baker/fixtures/text-shaders.toml adapters/mdux-vulkan-winit/shaders/generated adapters/mdux-vulkan-winit/shaders/generated/report.json
cargo run --locked -q -p hello_world -- --headless-smoke
cargo run --locked -q -p class_c_monitor -- --headless-smoke
```

## Hello World Vulkan text path

- `examples/hello_world/hello_world.medui` is the entire application-specific content; `examples/hello_world/build.rs` compiles it via `mdux-build`'s `MeduiScreen` into generated screen metadata, brought into scope with `mdux::include_medui_screen!()`.
- `mdux::screen_text::ScreenTextLayout` (in `crates/mdux`) resolves the screen's approved text into glyph draw commands — this is generic, screen-agnostic logic reused by every application.
- `adapters/mdux-vulkan-winit` owns everything platform-specific: it uploads the glyph atlas and renders textured quads using shaders precompiled to SPIR-V and committed under `adapters/mdux-vulkan-winit/shaders/generated/` (see `tools/mdux-shader-baker`), so applications need no `ash`/`winit`/`shaderc` dependency of their own.
- Use `cargo run -p hello_world -- --auto-close-ms=1000` to smoke-test the actual Vulkan text overlay path when a system Vulkan loader is available.
- `cargo run -p hello_world -- --headless-smoke` is still useful for non-graphical environments, but it intentionally skips the windowed Vulkan text-rendering path.

## Architecture decision records

- Text subsystem ADRs live under `docs/adr/ADR-001` through `ADR-004`.
- Framework architecture ADRs continue with:
  - `ADR-005`: pure-Rust project boundary and dependency policy
  - `ADR-006`: Vulkan versus Vulkan SC profile strategy
  - `ADR-007`: ownership and lifecycle of compliance evidence and generated artifacts
  - `ADR-008`: deterministic MedUI DSL boundary
  - `ADR-009`: MedUI compilation and generated artifacts
  - `ADR-010`: MedUI i18n and text-budget policy
  - `ADR-011`: MedUI safety-monitor and VulkanViewport contract
  - `ADR-012`: presentation adapter crates, the `adapters/` directory, and shader artifact evidence
  - `ADR-013`: host preview of Vulkan SC profiles and the bounded realtime contract
- Host-only third-party tooling used for the default Roboto bake path is tracked in `docs/governance/soup-register.toml`.

## Default Roboto asset governance

- The default approved source asset lives under `assets/fonts/roboto/` and includes the vendored `Roboto-Regular.ttf`, `font-manifest.toml`, `provenance.toml`, and Apache-2.0 notice material (`LICENSE`, `NOTICE`, upstream readmes).
- `assets/fonts/roboto/font-manifest.toml` is the source of truth for asset identity, digest pinning, and future Yocto-facing install and license fields (`package_name`, `install_subdir`, `license_expression`, `lic_files`, `source_uri`).
- `generated/fonts/roboto-regular-16px/` contains deterministic generated artifacts (`package.json`, `report.json`) for the approved Roboto fixture. These files are evidence outputs and must be regenerated with `tools/mdux-font-baker/`, not edited by hand.
- `tools/mdux-font-baker/` is host-only authoring tooling. Its SOUP dependencies stay outside the regulated runtime and outside future Yocto target images; only the reviewed source asset, notices, and generated package outputs cross into packaging or release evidence.

## Safety-critical text rendering

- Full Unicode, shaping, and bidi are handled offline for approved/localized strings.
- The runtime path only consumes immutable compiled text packages and bounded numeric templates.
- Font fallback, shaping, and atlas generation stay in the host-side authoring boundary so the rendering path remains deterministic and allocation-free.
