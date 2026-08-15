# ADR-011: MedUI safety-monitor and VulkanViewport contract

This local record implements the safety annotation and golden-reference rules of
[`MEDUI-DEC-004`](https://github.com/Compliatory/MedUI/blob/c8cc45ecec2f2dfd84940b9efc17c613e691cc0d/decisions/MEDUI-DEC-004-safety-and-goldens.md).
Its ADR number remains permanent and local.

- **Status:** Accepted
- **Date:** 2026-05-31
- **Related issues:** #6, #7, #8

## Context

The DSL memo requires compile-time golden-reference generation for CV-based safety checks and a dedicated primitive for reserving imaging regions in the UI.

## Decision

- `@safety_critical(cv_check: [...])` causes the compiler to emit a static golden-reference entry containing:
  - node id
  - resolved bounds
  - optional text key
  - optional color token
  - requested CV checks
- For `TrustSC`, safety-critical UI elements must still bind an explicit `requirement` identifier so UI traceability remains compatible with the current governance model.
- `VulkanViewport` compiles into a reserved region descriptor only; it does not embed arbitrary render logic in the UI package.

## Consequences

- Safety-critical annotations become auditable data, not comments.
- The DSL preserves the existing requirement-traceability chain.
- Direct imaging integration remains explicit and narrow, which avoids turning `VulkanViewport` into an escape hatch for uncontrolled behavior.
