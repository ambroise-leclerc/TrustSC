# Software Design Description (SDD) — TrustSC

> Filled-in example for TrustSC itself. See
> [`software_development_file/templates/IEC_62304/SDD.md`](../../templates/IEC_62304/SDD.md) for the
> blank template.

## Document control

- **Software item(s) covered:** the governed crates listed in the SAD's crate map (see
  [`SAD.md`](SAD.md) §2)
- **Version:** see `Cargo.lock`

## 1. Purpose and scope

This SDD details the internal design of TrustSC's governed crates, one level below the
architectural interfaces described in [`SAD.md`](SAD.md).

## 2. Detailed design per software unit

> `IEC 62304:2006 §5.4.1 Refine the software architecture into a detailed design`

### Unit: `trustsc-core`
- **Responsibility:** device metadata (`DeviceContext`), `SafetyClass` (B/C only — no Class A),
  `DeterminismPolicy`, `ValidationError`/`MduxResult`. Everything else in the workspace builds on it.

### Unit: `trustsc-governance`
- **Responsibility:** `Requirement`/`Hazard`/`VerificationCase`/`ProblemReport`, the `AuditEvent`
  trail, and `ComplianceProgram`, which ties requirements to verifications and exports a trace
  matrix (`trace_rows()`/`trace_matrix_export()`).

### Unit: `trustsc-ui`
- **Responsibility:** Vulkan/Vulkan SC UI policy — `UiSdkConfig`, `GraphicsProfile`,
  `MedicalUiRuntime`, deterministic `FrameStatistics`, and the `CompiledScreenPackage`/`CompiledNode`
  types consumed by generated MedUI DSL output.

### Unit: `trustsc-text-authoring` / `trustsc-text-runtime`
- **Responsibility:** full Unicode/shaping/bidi handling entirely offline
  (`compile_text_package`, host-side); the runtime side (`TextRuntime`, `GlyphDrawCommand`) is a
  no-allocation consumer of the pre-compiled, immutable `TextPackage` — no shaping or fallback logic
  runs on-device ([ADR-001](../../../docs/adr/ADR-001-safety-critical-text-rendering-architecture.md)).

### Unit: `trustsc-ml-runtime`
- **Responsibility:** `Classifier1D<'a, MAX_UNITS, MAX_OUT>` — a zero-allocation,
  `#![forbid(unsafe_code)]` inference engine using strictly-ordered scalar arithmetic (no SIMD, no
  FMA). `new()` re-runs every baked golden self-test vector and fails closed on any bit-mismatch
  ([ADR-017](../../../docs/adr/ADR-017-zero-soup-ml-inference-pipeline.md)).

### Unit: `trustsc` (facade)
- **Responsibility:** re-exports the above plus `FrameworkBuilder`/`Framework`,
  `screen_text::ScreenTextLayout`, the standard Roboto text package, the ML types, and the
  `include_medui_screen!`/`include_model!` macros.

## 3. Interface detailed design

> `IEC 62304:2006 §5.4.2 Develop a detailed design for interfaces`

`FrameworkBuilder::with_screen(&'static CompiledScreenPackage)` derives a `UiComponent` per
requirement-bearing screen node automatically, resolving its label from the standard approved text
package — the concrete mechanism letting an application skip hand-writing `UiComponent`s for its
screen while still satisfying `ComplianceProgram`'s requirement-linkage check (see
[`docs/iec62304/02-development-planning-and-requirements.md §5.2.5`](../../../docs/iec62304/02-development-planning-and-requirements.md#525-requirements-approval)).

## 4. Detailed design verification

> `IEC 62304:2006 §5.4.3 Verify the detailed design`

Each governed crate carries its own `cargo test` suite; `.github/workflows/ci.yml` runs
`cargo test --locked --quiet` across the whole workspace on every push. `--verify-ui` additionally
exercises rendered UI output against compiled bounds
([ADR-016](../../../docs/adr/ADR-016-automated-ui-verification-and-manual-generation.md)).

## Justification records

```json
{
  "justification_id": "JUS-002",
  "standard": "IEC 62304",
  "clause_ref": "IEC 62304:2006 §5.4.1 Refine the software architecture into a detailed design",
  "rationale": "trustsc-ml-runtime's Classifier1D re-runs baked golden self-test vectors at construction and fails closed on any bit-mismatch, giving the unit-level detailed design a runtime self-check beyond what a build-time test suite alone provides.",
  "evidence_refs": [
    "docs/adr/ADR-017-zero-soup-ml-inference-pipeline.md",
    "crates/trustsc-ml-runtime"
  ]
}
```
