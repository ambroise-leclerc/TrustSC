# MedUI DSL build integration

## Build flow (recommended: `mdux-build`)

Most applications should use the `mdux-build` helper crate instead of calling
`mdux-ui-dsl-authoring` directly:

```rust
// build.rs
fn main() -> Result<(), Box<dyn std::error::Error>> {
    mdux_build::MeduiScreen::new("hello_world.medui")
        .surface(800, 480)
        .compile()
}
```

```rust
// src/main.rs (or lib.rs)
mdux::include_medui_screen!();
// exposes medui_screen::screen() -> &'static mdux::CompiledScreenPackage
// and medui_screen::primary_text_node_id() -> &'static str
```

1. author `hello_world.medui`
2. `build.rs` recompiles when the file changes (`MeduiScreen::compile` emits
   `cargo:rerun-if-changed`)
3. `MeduiScreen::compile` resolves the approved text package via
   `mdux::default_standard_text_package()` and `mdux::default_display_text_package()` (the
   ADR-013 pair — `NumericDisplay` budgets resolve in the display package) and calls
   `mdux-ui-dsl-authoring` to parse, validate,
   and compile the file — every `t("key")` reference is checked against all approved locales, and
   the compiler rejects any component whose allocated bounds are smaller than the widest approved
   translation
4. the compiler writes a generated Rust module to `$OUT_DIR/mdux_medui_screen.rs`, with every type
   qualified against `::mdux` so the including file needs no `use` statements
5. `mdux::include_medui_screen!()` includes that module as `medui_screen`, exposing `screen()` and
   `primary_text_node_id()` alongside the lower-level `GENERATED_MEDUI_PACKAGE` /
   `GENERATED_PRIMARY_TEXT_NODE_ID` items

## Manual / advanced flow

`mdux_ui_dsl_authoring::compile_medui_file_to_rust_module` remains available directly for callers
that need a non-default output path, a custom `crate_path` (e.g. crate-internal tests that
re-export the same types locally instead of depending on `mdux`), or to compile from an
already-loaded `TextPackage` rather than the standard one. `mdux-build`'s `MeduiScreen` is a thin,
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
