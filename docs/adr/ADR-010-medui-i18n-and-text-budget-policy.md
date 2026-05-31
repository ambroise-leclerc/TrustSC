# ADR-010: MedUI i18n and text-budget policy

- **Status:** Accepted
- **Date:** 2026-05-31
- **Related issues:** #6, #8, #9

## Context

The DSL memo requires hardcoded product strings to be forbidden and text rendering to remain deterministic and certifiable.

## Decision

- DSL text properties use `t("key")` references only.
- `build.rs` resolves DSL text keys against the existing approved text package at compile time.
- The compiler measures every approved locale/run for a given text key and rejects any screen whose allocated bounds are smaller than the widest approved translation.
- The runtime path uses the DSL-derived text key to select the compiled text run instead of a hardcoded run identifier in the demo.

## Consequences

- The `hello_world` demo proves that the DSL controls text selection and placement.
- Text overflow is now rejected during `.medui` compilation instead of being deferred to runtime behavior.
- The existing text package stays the single source of truth for approved strings and compiled glyph runs.
