# Architecture

This page explains how the TrustSC workspace is organized and why, so a reviewer or a new
contributor can find the crate responsible for a given piece of behavior. For the design
rationale behind each boundary, see the [ADR index](adr/README.md). For how this architecture
maps onto IEC 62304 Class B/C review scope, see
[Regulatory compliance](regulatory-compliance.md).

## Three trust zones

Every crate in the workspace lives in one of three directories, and that directory *is* its
trust-zone declaration ([ADR-005](adr/ADR-005-pure-rust-project-boundary-and-dependency-policy.md),
[ADR-012](adr/ADR-012-presentation-adapter-crates-and-shader-artifacts.md)):

- **`crates/` — governed crates.** Pure Rust, `#![forbid(unsafe_code)]`, no FFI types or native
  SDK handles in any public API. May depend only on each other or on version-pinned, reviewed
  crates recorded in the SOUP register. This is the small, auditable core.
- **`adapters/` — edge adapters.** The only place `unsafe` and native bindings (`ash`,
  `ash-window`, `raw-window-handle`, `winit`) are allowed — and only if every public function
  takes or returns owned Rust data already defined by a governed crate (`Framework`,
  `CompiledScreenPackage`, `ScreenTextLayout`, ...). No foreign handle crosses back into a
  governed crate. `adapters/trustsc-vulkan-winit` is currently the only crate here: it owns the
  Vulkan instance/device/swapchain/pipeline, glyph-atlas upload, and the winit event loop for
  every example.
- **`tools/` — host-only tooling.** May use additional reviewed third-party crates to bake
  generated evidence artifacts, but is never linked into a device/runtime crate and never ships
  in a runtime artifact. Tracked in [`docs/governance/soup-register.toml`](governance/soup-register.toml),
  not treated as part of the validated software item.

When adding a dependency, the first question is which zone the crate lives in — that determines
whether the dependency is even permissible without a new ADR.

## Crate map

| Crate | Zone | Role |
|---|---|---|
| `trustsc-core` | governed | Device metadata (`DeviceContext`), `SafetyClass` (B/C), `DeterminismPolicy`, `ValidationError`/`TrustScResult`. |
| `trustsc-governance` | governed | `Requirement`/`Hazard`/`VerificationCase`/`ProblemReport`, `AuditEvent` trail, `ComplianceProgram` tying requirements to verifications and exporting a trace matrix. |
| `trustsc-ui` | governed | Vulkan/Vulkan SC UI policy: `UiSdkConfig`, `GraphicsProfile`, `MedicalUiRuntime`, deterministic `FrameStatistics`, `CompiledScreenPackage`/`CompiledNode` types. |
| `trustsc-ui-verify` | governed | Offscreen rendering / rendered-truth verification engine behind `--verify-ui` ([ADR-016](adr/ADR-016-automated-ui-verification-and-manual-generation.md)). |
| `trustsc-ui-dsl-authoring` | host-side, feeds governed | Host-side compiler for the `.medui` DSL, used from `build.rs`. |
| `trustsc-text-schema` / `trustsc-text-authoring` / `trustsc-text-runtime` | governed | Text pipeline: shared manifest/compiled-package schema, host-side font intake and atlas compilation, no-allocation runtime consumer. |
| `trustsc-image-schema` | governed | Immutable compiled image-package schema for governed logo/icon assets. |
| `trustsc-ml-schema` / `trustsc-ml-authoring` / `trustsc-ml-runtime` | governed | ML pipeline: shared `ModelPackage` contract, host-side safetensors import and compilation, zero-allocation `Classifier1D` inference engine ([ADR-017](adr/ADR-017-zero-soup-ml-inference-pipeline.md)). |
| `trustsc` | governed | Facade re-exporting the above, `FrameworkBuilder`/`Framework`, `screen_text::ScreenTextLayout`, the standard Roboto text package, and the `include_medui_screen!`/`include_model!` macros. |
| `trustsc-build` | host-side, feeds governed | Build-script helper wrapping `trustsc-ui-dsl-authoring`, the `.medui`/`ModelPackage`/scenario compilers. |
| `adapters/trustsc-vulkan-winit` | edge adapter | The only crate depending on `ash`/`ash-window`/`raw-window-handle`/`winit`; owns the Vulkan/winit runtime for every example. |
| `tools/trustsc-font-baker`, `tools/trustsc-image-baker`, `tools/trustsc-shader-baker`, `tools/trustsc-ml-baker` | host-only | `bake`/`verify` CLIs that compile source assets (fonts, images, GLSL shaders, ML weights) into committed, byte-verified `package.json`/`report.json` evidence. |

## The evidence-generation pattern

A recurring shape ([ADR-007](adr/ADR-007-compliance-evidence-and-generated-artifact-ownership.md))
underlies every asset pipeline in this repo: a host-only `tools/*-baker` binary consumes a
reviewed source input (a font, a shader, a set of model weights) plus a recipe file, and produces
two committed artifacts — `package.json` (the data itself, deterministically serialized) and
`report.json` (a SHA-256 digest, the tool version, and the options used). CI then runs only the
tool's `verify` subcommand, which re-derives the artifacts from the same recipe and checks the
result is byte-identical to what's committed. This means:

- CI never needs `shaderc`, `fontdue`, or any authoring-time dependency installed — only the
  governed/adapter crates and the four `verify` commands.
- A change to committed evidence is visible as a diff, with the tool and options that produced it
  recorded alongside.
- Swapping the underlying source data (e.g. a manufacturer's own ML weights for the demonstrator
  weights) never requires touching the governed or adapter code that consumes the evidence.

## Continuous integration

`.github/workflows/ci.yml` runs on `push`, `pull_request`, and manual dispatch: it builds the
Linux workspace with locked dependencies, runs the full test suite, lints regulatory citations,
byte-verifies every committed evidence artifact (Roboto at three sizes, the Acme-logo image,
SPIR-V shaders, the `eeg-demo` ML model), self-tests the MedUI Studio render bridge, exercises
`hello_world` and `class_c_monitor` through `--headless-smoke`, then through `--verify-ui`
(ADR-016 — [operator guide](verification/ui-verification.md)) and uploads
`generated/verification/` as an artifact. The exact, kept-current replay command set lives in
[AGENTS.md's CI-replay block](../AGENTS.md#replaying-ci-locally) rather than duplicated here.

## Default Roboto asset governance

- The default approved source asset lives under `assets/fonts/roboto/`: the vendored
  `Roboto-Regular.ttf`, `font-manifest.toml`, `provenance.toml`, and Apache-2.0 notice material.
- `assets/fonts/roboto/font-manifest.toml` is the source of truth for asset identity, digest
  pinning, and Yocto-facing install/license fields.
- `generated/fonts/roboto-regular-16px/` holds deterministic generated artifacts (`package.json`,
  `report.json`) for the approved Roboto fixture — evidence outputs, regenerated with
  `tools/trustsc-font-baker`, never hand-edited.
- `tools/trustsc-font-baker/` is host-only authoring tooling; its SOUP dependencies stay outside the
  regulated runtime and outside any future Yocto target image.

## Safety-critical text rendering

- Full Unicode, shaping, and bidi are handled offline for approved/localized strings.
- The runtime path only consumes immutable compiled text packages and bounded numeric templates.
- Font fallback, shaping, and atlas generation stay in the host-side authoring boundary, so the
  rendering path remains deterministic and allocation-free.
