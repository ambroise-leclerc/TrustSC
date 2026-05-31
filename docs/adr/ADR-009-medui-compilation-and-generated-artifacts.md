# ADR-009: MedUI compilation and generated artifacts

- **Status:** Accepted
- **Date:** 2026-05-31
- **Related issues:** #6, #7, #8

## Context

The DSL must produce evidence that can be audited, diffed, and consumed by runtime code without parsing the original `.medui` source.

## Decision

- `examples/hello_world/build.rs` compiles `.medui` source into a Rust module stored in `OUT_DIR`.
- The host-side compiler lives in `crates/mdux-ui-dsl-authoring` and is used as a build dependency only.
- Generated artifacts contain:
  - static screen layout data
  - node metadata required by the runtime
  - golden-reference entries for `@safety_critical` nodes
- Generated Rust modules are treated as build output and evidence, not as hand-edited source.

## Consequences

- A broken or missing `.medui` file fails the build instead of degrading silently.
- Runtime crates remain free of DSL parsing logic.
- Future work may add generated C headers/tables for external safety monitors without changing the core source-of-truth model.
