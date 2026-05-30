# MduX-rust

Medical-device manufacturer framework with Class B/Class C compliance modeling and a Vulkan / Vulkan SC-oriented UI SDK.

## Workspace layout

- `crates/mdux-core`: device metadata, safety classes, deterministic runtime policy
- `crates/mdux-governance`: requirements, hazards, verifications, audit trail, trace matrix export
- `crates/mdux-ui`: Vulkan / Vulkan SC UI policy and deterministic frame model
- `crates/mdux-text-schema`: shared manifests and immutable compiled text-package schema
- `crates/mdux-text-authoring`: host-side font intake, deterministic atlas compilation, and asset tooling
- `crates/mdux-text-runtime`: no-allocation runtime text command generation from approved packages
- `crates/mdux`: thin facade for building complete framework instances
- `examples/hello_world`: smallest out-of-the-box smoke demo
- `examples/class_b_device`: Class B Vulkan example
- `examples/class_c_vulkansc_device`: Class C Vulkan SC example

## Commands

```bash
source $HOME/.cargo/env
cd MduX-rust

# build everything
cargo build

# run all tests
cargo test

# run a single test
cargo test builds_hello_world_demo_through_public_api

# run the shortest demo (opens a Vulkan window)
cargo run -p hello_world

# run it and close automatically after one second
cargo run -p hello_world -- --auto-close-ms=1000

# run the same smoke path without a window
cargo run -p hello_world -- --headless-smoke

# run the richer examples
cargo run -p class_b_device
cargo run -p class_c_vulkansc_device

# inspect the text-asset pipeline tooling
cargo run -p mdux-text-authoring --bin mdux-textc -- describe-pipeline
```

The default `hello_world` example now opens a real Vulkan window. Use `--headless-smoke` when validating the framework in a non-graphical environment.

## Continuous integration

- `.github/workflows/ci.yml` runs on `push` and `pull_request` for `main` and `develop`.
- The workflow validates the Linux workspace with locked dependencies, runs the full test suite, verifies the committed Roboto artifacts, and exercises `hello_world` through `--headless-smoke`.
- Replay the same checks locally with:

```bash
source $HOME/.cargo/env
cd MduX-rust
cargo build --locked --workspace
cargo test --locked --quiet
cargo run --locked -q -p mdux-font-baker -- verify tools/mdux-font-baker/fixtures/roboto-demo.toml generated/fonts/roboto-regular-16px/package.json generated/fonts/roboto-regular-16px/report.json
cargo run --locked -q -p hello_world -- --headless-smoke
```

## Hello World Vulkan text path

- `examples/hello_world/src/hello_text.rs` embeds a deterministic text package for the approved string `Hello World !`.
- `examples/hello_world/src/vulkan_window.rs` uploads that atlas and renders textured glyph quads with the example's compiled text shaders.
- Use `cargo run -p hello_world -- --auto-close-ms=1000` to smoke-test the actual Vulkan text overlay path.
- `cargo run -p hello_world -- --headless-smoke` is still useful for non-graphical environments, but it intentionally skips the windowed Vulkan text-rendering path.

## Architecture decision records

- Text subsystem ADRs live under `docs/adr/ADR-001` through `ADR-004`.
- Framework architecture ADRs continue with:
  - `ADR-005`: pure-Rust project boundary and dependency policy
  - `ADR-006`: Vulkan versus Vulkan SC profile strategy
  - `ADR-007`: ownership and lifecycle of compliance evidence and generated artifacts
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
