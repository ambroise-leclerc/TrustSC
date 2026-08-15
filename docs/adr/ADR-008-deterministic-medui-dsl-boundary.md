# ADR-008: Deterministic MedUI DSL boundary

- **Status:** Accepted
- **Date:** 2026-05-31
- **Related issues:** #6, #7, #8

## Shared decision identity

This TrustSC decision implements
[`MEDUI-DEC-001`](https://github.com/Compliatory/MedUI/blob/c8cc45ecec2f2dfd84940b9efc17c613e691cc0d/decisions/MEDUI-DEC-001-build-time-compilation.md)
and the closed-language part of `MEDUI-DEC-002`. ADR-008 remains the permanent local identity;
cross-repository citations use the shared identifier.

## Context

`TrustSC` needs a declarative UI authoring language for safety-critical medical screens without introducing runtime interpretation, dynamic layout solving, or uncontrolled text rendering into the regulated runtime path.

## Decision

- `.medui` files are **authored source**, not runtime assets.
- The DSL is compiled at build time into static Rust data structures.
- The runtime consumes generated `CompiledScreenPackage` data only.
- The view layer remains intentionally narrow:
  - static screens only
  - `Vertical` / `Horizontal` layout for the first slice
  - approved built-in components only
  - no loops, no conditionals, no recursion, no embedded scripting
- Product strings must be referenced via `t("key")`.

## Consequences

- Authoring complexity stays on the host/build side.
- Runtime determinism is preserved because layout and widget selection are fixed before execution.
- The first slice is deliberately incomplete by design; richer widgets and full i18n sizing remain follow-up work.
