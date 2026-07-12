# ADR-012: Presentation adapter crates, the `adapters/` directory, and shader artifact evidence

## Status

Accepted

## Context

`examples/hello_world` currently hand-writes its entire Vulkan/winit renderer (~1,800 lines) inline in
the example binary: instance/device/swapchain setup, glyph-atlas upload, vertex generation, the event
loop, and GLSL-to-SPIR-V compilation via `shaderc` in its `build.rs`. None of that is specific to the
hello-world screen; every windowed TrustSC application would need to copy it verbatim.

ADR-005 already anticipates this: it permits `unsafe` code, native SDK wrappers, and FFI-facing types
"only in edge adapters such as platform examples, harnesses, or **future integration crates** that
translate into owned Rust data before crossing into the governed API boundary." What ADR-005 does not
yet specify is where such a reusable integration crate lives, what it is named, and how its dependencies
and generated build artifacts (compiled shaders) are tracked. This ADR settles those three points so the
renderer can be extracted into a shared crate without reopening ADR-005.

## Decision

1. **`adapters/` is a third trust-zone directory**, alongside `crates/` (governed, `#![forbid(unsafe_code)]`,
   no native handles in public APIs) and `tools/` (host-only, never linked into device/runtime crates).
   Crates under `adapters/` are edge adapters in the ADR-005 sense: they may use `unsafe`, native SDK
   bindings (`ash`, `winit`, `raw-window-handle`), and FFI-facing types internally, but every public
   function must take or return owned Rust data already defined by a governed crate (e.g. `trustsc::Framework`,
   `trustsc::CompiledScreenPackage`, `trustsc::screen_text::ScreenTextLayout`). No `ash::vk` handle, `winit`
   type, or other foreign ABI type may appear in a governed crate's public API as a result of adding an
   adapter — the translation happens inside the adapter, not at its boundary with `crates/`.
2. **The first adapter crate is named `trustsc-vulkan-winit`**, at `adapters/trustsc-vulkan-winit`. The name
   states the exact platform stack it adapts (Vulkan 1.x windowed rendering via `winit`), leaving room for
   future siblings such as a Vulkan SC offline-pipeline adapter without overloading one crate's scope.
3. **Committed SPIR-V binaries are generated evidence under the ADR-007 model.** The GLSL shader sources
   are the reviewed input (already app-agnostic text-rendering shaders, currently in
   `examples/hello_world/shaders/` and planned to move to `adapters/trustsc-vulkan-winit/shaders/` once that
   crate exists). A new host-only tool, `tools/trustsc-shader-baker`, owns
   `bake`/`verify` subcommands that compile GLSL to SPIR-V with pinned `shaderc` options and record a
   `report.json` (per-artifact SHA-256, `shaderc` version, compile options) next to the committed `.spv`
   files, mirroring the `tools/trustsc-font-baker` → `generated/fonts/roboto-regular-16px/` pattern. CI runs
   `verify`, not `bake`, so application builds and CI never need `shaderc` themselves — only
   `tools/trustsc-shader-baker` does.
4. **SOUP register scope widens to cover presentation-adapter dependencies.** `ash`, `ash-window`, and
   `raw-window-handle` are registered as pinned adapter-library dependencies (they compile into
   `adapters/trustsc-vulkan-winit` and ship in any binary that links it); `winit` likewise. `shaderc` is
   registered as tools-only, exactly like the font-baker's `fontdue`/`toml` entries, since it never
   compiles into `adapters/trustsc-vulkan-winit` or any application.

## Consequences

- The reusable renderer can be extracted from `examples/hello_world` into `adapters/trustsc-vulkan-winit`
  without a governed crate gaining a graphics dependency, and without ambiguity about where such a crate
  should live or what it may do.
- `examples/hello_world`'s `Cargo.toml` can drop `shaderc` as a build dependency entirely once shaders are
  committed evidence, removing the heaviest build-time dependency (a C++ toolchain) from the example.
- Future platform adapters (e.g. a Vulkan SC variant) have a settled naming and boundary convention to
  follow instead of re-deriving one per crate.
- The SOUP register's `scope` field is broadened from "host-only-font-tooling"; reviewers should read
  each entry's `integration_path` and `runtime_deployment` fields rather than relying on the top-level
  scope description alone.
