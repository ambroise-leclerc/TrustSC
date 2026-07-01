# CLAUDE.md

This file provides guidance to Claude Code (claude.ai/code) when working with code in this repository.

## Repository overview

MduX-rust is a pure-Rust replacement for the original C++ MduX framework: a medical-device UI SDK with
IEC 62304 Class B / Class C compliance modeling built in, targeting Vulkan (Class B) and Vulkan SC (Class C).
It is a separate project from `MduX`, `SpecLab`, and `mddlog` under `Projets_MduX/` — the root-level
`Projets_MduX/CLAUDE.md` describes those C++/CMake projects and does not apply here.

## Commands

```bash
source $HOME/.cargo/env

# build everything
cargo build

# run all tests
cargo test

# run a single test
cargo test builds_hello_world_demo_through_public_api

# run the shortest demo (opens a Vulkan window; requires a system Vulkan loader)
cargo run -p hello_world
cargo run -p hello_world -- --auto-close-ms=1000   # auto-close after 1s
cargo run -p hello_world -- --headless-smoke       # no window, for CI / non-graphical hosts

# richer examples
cargo run -p class_b_device
cargo run -p class_c_vulkansc_device

# text-asset pipeline tooling
cargo run -p mdux-text-authoring --bin mdux-textc -- describe-pipeline
```

### Replaying CI locally

`.github/workflows/ci.yml` runs on every push/PR/manual dispatch. Reproduce it exactly with:

```bash
cargo build --locked --workspace
cargo test --locked --quiet
cargo run --locked -q -p mdux-font-baker -- verify tools/mdux-font-baker/fixtures/roboto-demo.toml generated/fonts/roboto-regular-16px/package.json generated/fonts/roboto-regular-16px/report.json
cargo run --locked -q -p hello_world -- --headless-smoke
```

Vulkan prerequisites (needed only for the windowed path, not for `--headless-smoke`):
- Linux: `sudo apt-get install libvulkan1 libvulkan-dev vulkan-tools`
- macOS: `brew install vulkan-loader molten-vk vulkan-tools` plus `VK_ICD_FILENAMES`/`DYLD_FALLBACK_LIBRARY_PATH` exports (see README.md)

## Architecture

### The governed vs. host-side boundary (ADR-005, ADR-012)

This is the central design constraint of the whole workspace. Crates split into three trust-zone
directories:

- **`crates/`  — governed crates** — `mdux-core`, `mdux-governance`, `mdux-ui`, `mdux`,
  `mdux-text-schema`, `mdux-text-authoring`, `mdux-text-runtime` (and `mdux-ui-dsl-authoring` /
  `mdux-build`, host-side but feeding the governed model) — are pure Rust, `#![forbid(unsafe_code)]`,
  depend only on each other or version-pinned, reviewable crates. No FFI types, native SDK handles, or
  bindgen output may appear in their public APIs.
- **`adapters/` — edge adapter crates** (ADR-012) — e.g. `mdux-vulkan-winit` — may use `unsafe`, native
  SDK bindings (`ash`, `ash-window`, `raw-window-handle`, `winit`), etc. internally, but every public
  function must take or return owned Rust data already defined by a governed crate (`Framework`,
  `CompiledScreenPackage`, `ScreenTextLayout`, ...) — no foreign handle may cross that boundary.
  Examples (`hello_world`, `class_b_device`, `class_c_vulkansc_device`) are also edge adapters in the
  ADR-005 sense, but should consume a reusable `adapters/` crate rather than hand-writing platform code.
- **`tools/` — host-only tooling** (e.g. `mdux-font-baker`, `mdux-shader-baker`) may use additional
  reviewed third-party crates (`shaderc`, `fontdue`, ...) to bake generated evidence artifacts. This
  tooling and its dependencies must never be linked into device/runtime crates or shipped in runtime
  artifacts — they are tracked in `docs/governance/soup-register.toml`, not treated as part of the
  validated software item.

When adding a dependency, first ask which zone the crate lives in — that determines whether the
dependency is even permissible without a new ADR.

### Crate map (`crates/`)

- `mdux-core` — device metadata (`DeviceContext`), `SafetyClass` (B/C), `DeterminismPolicy`,
  `ValidationError`/`MduxResult`. Everything else builds on this.
- `mdux-governance` — `Requirement`/`Hazard`/`VerificationCase`/`ProblemReport`, `AuditEvent` trail,
  `ComplianceProgram` that ties requirements to verifications and exports a trace matrix.
- `mdux-ui` — Vulkan/Vulkan SC UI policy: `UiSdkConfig`, `GraphicsProfile`, `MedicalUiRuntime`,
  deterministic `FrameStatistics`, `CompiledScreenPackage`/`CompiledNode` types consumed by generated
  DSL output.
- `mdux-ui-dsl-authoring` — host-side compiler for the `.medui` DSL (parses `Screen`/`Vertical`/
  `Horizontal`/`CriticalButton`/`VulkanViewport`/`@safety_critical`/`t("key")`), used from `build.rs`.
- `mdux-text-schema` — shared manifest types and the immutable compiled text-package schema
  (`TextPackage`, `CompiledGlyph`, `TextureAtlas`, etc.) — the contract between authoring and runtime.
- `mdux-text-authoring` — host-side font intake, deterministic glyph-atlas compilation
  (`compile_text_package`), font fingerprinting; ships the `mdux-textc` binary.
- `mdux-text-runtime` — no-allocation runtime consumer of compiled text packages
  (`TextRuntime`, `GlyphDrawCommand`). This is the only text code that runs on-device.
- `mdux` — thin facade re-exporting the above plus `FrameworkBuilder`/`Framework`, the standard
  Roboto text package (`standard_text.rs`), and the `hello_world` demo builder used by tests.

`FrameworkBuilder` (`crates/mdux/src/lib.rs`) is the composition root: it wires a `DeviceContext` +
`ComplianceProgram` + `UiSdkConfig` + `UiComponent`s together, cross-validates them (e.g. Class C
devices are rejected unless the UI config uses the Vulkan SC profile, UI components must reference
requirements that actually exist in the compliance program), and only then produces a `Framework`.

### The MedUI DSL (ADR-008/009/010/011, `docs/dsl/`)

`.medui` files are a deterministic, build-time-only UI description language — not parsed at runtime.
Flow (see `docs/dsl/build-integration.md`):

1. author a `.medui` file (see `examples/hello_world/hello_world.medui`)
2. the example's `build.rs` invokes `mdux-ui-dsl-authoring` to parse/validate it
3. every `t("key")` reference is checked against the approved text package across *all* approved
   locales — a component is rejected at compile time if its allocated bounds are too small for the
   widest approved translation
4. the compiler emits a generated Rust module (`OUT_DIR`) exposing a `CompiledScreenPackage`,
   including golden-reference entries for any `@safety_critical` node
5. the runtime only ever consumes the generated `CompiledScreenPackage` — no DSL parsing or dynamic
   layout solving happens on-device

This narrow, compile-time-checked boundary is what keeps the runtime deterministic and allocation-free
while still giving humans/LLMs a structured way to author screens.

### Text pipeline (ADR-001–004, `mdux-text-*`)

Full Unicode/shaping/bidi handling is entirely offline (`mdux-text-authoring`). The runtime
(`mdux-text-runtime`) only ever consumes pre-compiled, immutable `TextPackage`s and bounded numeric
templates — no shaping, fallback, or atlas generation on-device. `examples/hello_world` demonstrates
the full path: `hello_text.rs` embeds the compiled package, `vulkan_window.rs` uploads the atlas and
renders textured glyph quads via `shaders/hello_text.{vert,frag}`.

### Font/asset governance

`assets/fonts/roboto/` holds the single approved source asset (vendored `Roboto-Regular.ttf` +
`font-manifest.toml` provenance/licensing metadata). `generated/fonts/roboto-regular-16px/` holds
derived, deterministic build evidence (`package.json`, `report.json`) — these are generated artifacts;
regenerate them via `tools/mdux-font-baker`, never hand-edit. The font baker itself is host-only
tooling and stays outside the regulated runtime boundary (see ADR-005 above).

## Notes for changes

- Governed crates keep `#![forbid(unsafe_code)]` — if a change seems to need `unsafe` or a native
  handle in one of them, it belongs in an edge adapter instead, or needs a new ADR.
- `Cargo.lock` is committed and CI builds with `--locked`; update the lockfile deliberately.
- ADRs under `docs/adr/` are the authoritative source for *why* a boundary exists — check them before
  proposing changes that would cross the governed/host-side line or alter the DSL's compile-time-only
  contract.
