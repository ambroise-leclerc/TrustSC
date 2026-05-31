# MedUI DSL overview

The MedUI DSL is a deterministic build-time language for authored medical UI screens in `MduX-rust`.

## Goals

- describe static UI trees declaratively
- compile to static runtime data
- generate golden-reference data for safety monitoring
- keep runtime free of DSL parsing and dynamic layout solving

## First implementation slice

The current slice is intentionally narrow:

- `Screen`
- root `Vertical` / `Horizontal` layout
- `CriticalButton`
- `VulkanViewport`
- `@safety_critical`
- `t("key")`
- enum-only `SystemEvent` bindings

## Boundary

- `.medui` is authored source
- `build.rs` compiles `.medui` into generated Rust
- the compiler validates text budgets against the approved text package before code generation
- runtime consumes `CompiledScreenPackage` only

This keeps the regulated runtime deterministic while still giving developers and LLMs a structured UI description format.
