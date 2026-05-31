# MedUI DSL build integration

## Build flow

1. author `hello_world.medui`
2. `examples/hello_world/build.rs` recompiles when the file changes
3. `mdux-ui-dsl-authoring` parses and validates the file
4. `build.rs` loads the approved text package and checks every `t("key")` against all approved locales
5. the compiler rejects any component whose allocated bounds are smaller than the widest approved translation
6. the compiler writes a generated Rust module to `OUT_DIR`
7. the example includes that module and uses the resulting `CompiledScreenPackage`

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
