# MedUI DSL build integration

## Build flow (recommended: `trustsc-build`)

Most applications should use the `trustsc-build` helper crate instead of calling
`trustsc-ui-dsl-authoring` directly:

```rust
// build.rs
fn main() -> Result<(), Box<dyn std::error::Error>> {
    trustsc_build::MeduiScreen::new("hello_world.medui")
        .surface(800, 480)
        .compile()
}
```

```rust
// src/main.rs (or lib.rs)
trustsc::include_medui_screen!();
// exposes medui_screen::screen() -> &'static trustsc::CompiledScreenPackage
// and medui_screen::primary_text_node_id() -> &'static str
```

1. author `hello_world.medui`
2. `build.rs` recompiles when the file changes (`MeduiScreen::compile` emits
   `cargo:rerun-if-changed`)
3. `MeduiScreen::compile` resolves the approved text package via
   `trustsc::default_standard_text_package()` and `trustsc::default_display_text_package()` (the
   ADR-013 pair — `NumericDisplay` budgets resolve in the display package) and calls
   `trustsc-ui-dsl-authoring` to parse, validate,
   and compile the file — every `t("key")` reference is checked against all approved locales, and
   the compiler rejects any component whose allocated bounds are smaller than the widest approved
   translation
4. the compiler writes a generated Rust module to `$OUT_DIR/trustsc_medui_screen.rs`, with every type
   qualified against `::trustsc` so the including file needs no `use` statements
5. `trustsc::include_medui_screen!()` includes that module as `medui_screen`, exposing `screen()` and
   `primary_text_node_id()` alongside the lower-level `GENERATED_MEDUI_PACKAGE` /
   `GENERATED_PRIMARY_TEXT_NODE_ID` items

## ML model build step (ADR-017)

`trustsc-build` also exposes a `ModelPackage` builder alongside `MeduiScreen`, for any application
embedding an on-device ML model (ADR-017) — independent of whether its screen has a `SignalTrace`
node: a model can drive a `NumericDisplay`/`StatusIndicator` with no trace in sight, and a screen
can have a `SignalTrace` with no model behind it at all.

```rust
// build.rs
fn main() -> Result<(), Box<dyn std::error::Error>> {
    trustsc_build::MeduiScreen::new("neurosense.medui").surface(1920, 1080).compile()?;
    trustsc_build::ModelPackage::new("../../generated/models/eeg-demo/package.json").compile()
}
```

```rust
// src/lib.rs
trustsc::include_model!();
// exposes medui_model::model() -> trustsc::ModelPackage
```

The same JSON-to-Rust codegen doctrine applies: `ModelPackage::compile` reads the committed,
already-baked-and-verified `generated/models/<id>/package.json` (produced by `tools/trustsc-ml-baker`
from a recipe — never from a raw Hugging Face download at build time) and transcribes it into a
generated `$OUT_DIR/trustsc_ml_model.rs`. Swapping which `package.json` this points at — a Hugging
Face demonstrator vs. a manufacturer's own clinically-qualified weights, both baked by the
identical pipeline — is the entire "weights are data" story: zero application source changes.

## Checking a `.medui` file without building

`tools/trustsc-medui-check` validates a single file and prints compiler diagnostics, for instant
feedback while hand-editing a screen without building an example app or waiting on a `build.rs`
run:

```sh
cargo run -p trustsc-medui-check -- path/to/screen.medui
```

Prints `OK <screen name> (<N> nodes)` and exits `0` on success; otherwise prints each diagnostic
(with a line number when the parser produced one — semantic errors caught during compilation,
like an unknown color token, don't carry one) to stderr and exits `1`. A screen with no `surface:`
pin is checked against the same 800×480 default `tools/trustsc-medui-studio` and
`examples/hello_world`'s own `build.rs` use.

## Manual / advanced flow

`trustsc_ui_dsl_authoring::compile_medui_file_to_rust_module` remains available directly for callers
that need a non-default output path, a custom `crate_path` (e.g. crate-internal tests that
re-export the same types locally instead of depending on `trustsc`), or to compile from an
already-loaded `TextPackage` rather than the standard one. `trustsc-build`'s `MeduiScreen` is a thin,
opinionated wrapper around this function plus `CompileOptions::new` — nothing it does is
unavailable to a caller who wires the lower-level API up by hand.

## Current generated outputs

- static screen metadata
- resolved bounds
- compile-time text-budget validation against the approved text package
- golden-reference entries for `@safety_critical` nodes

## Current demo proof

The `hello_world` Vulkan path now uses the generated screen package to:

- resolve the text key
- select the compiled text run
- position the text from DSL-derived bounds
