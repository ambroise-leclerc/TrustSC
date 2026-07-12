# Usability Engineering File — TrustSC

> Filled-in example for TrustSC itself. See
> [`software_development_file/templates/IEC_62366/Usability_Engineering_File.md`](../../templates/IEC_62366/Usability_Engineering_File.md)
> for the blank template, and [`docs/iec62366/README.md`](../../../docs/iec62366/README.md) for the
> underlying clause-by-clause guidance.

## Document control

- **Product / software item:** TrustSC's UI layer (`trustsc-ui`, the MedUI DSL, `trustsc-text-*`,
  `adapters/trustsc-vulkan-winit`)
- **Scope note:** TrustSC provides UI *building blocks* and build-time enforcement, not a finished
  device's usability engineering file — a manufacturer's actual use specification, evaluation
  results, and summative testing are theirs to conduct and document. This file states what
  TrustSC's mechanisms can feed into that process.

## 1. Use specification

> `IEC 62366-1:2015 §5.1 Use specification`

Not applicable to TrustSC as an SDK — intended use, patient population, and use environment are
device-specific and belong to the manufacturer's own use specification.

## 2. Frequently used functions and hazard-related use scenarios

> `IEC 62366-1:2015 §5.2 Identify frequently used functions and hazard-related use scenarios`

TrustSC's `@safety_critical` MedUI annotation (see
[`docs/dsl/safety-monitor-contract.md`](../../../docs/dsl/safety-monitor-contract.md) and
[ADR-011](../../../docs/adr/ADR-011-medui-safety-monitor-and-vulkan-viewport-contract.md)) is the
mechanism a manufacturer uses to mark which UI elements correspond to hazard-related use scenarios in
their own analysis. ADR-011 states that a safety-critical UI element should also bind an explicit
`requirement` identifier so its traceability stays compatible with the governance model — but as of
this writing the `.medui` compiler (`crates/trustsc-ui-dsl-authoring`) does not yet enforce that binding
at build time: `@safety_critical(cv_check: [...])` and a node's `requirement` are independent
attributes, and a `@safety_critical` node with no `requirement` currently compiles without error. A
manufacturer relying on this link being build-enforced should verify that against the compiler
version they use, not assume it from this document.

## 3. User interface specification

> `IEC 62366-1:2015 §5.3 User interface specification`

Authored as `.medui` source files (see [`docs/dsl/overview.md`](../../../docs/dsl/overview.md)) —
a deterministic, build-time-only UI description language. `examples/hello_world/hello_world.medui`
is a minimal worked specification; `examples/class_c_monitor` is a fuller one (NeuroSense 500,
1920×1080, 156 lines, 6 golden references).

## 4. User interface evaluation plan

> `IEC 62366-1:2015 §5.4 User interface evaluation plan`

`--verify-ui` ([ADR-016](../../../docs/adr/ADR-016-automated-ui-verification-and-manual-generation.md))
provides automated, repeatable *rendering-correctness* evaluation (does approved content render
within its compiled bounds, on the CI lavapipe rasterizer and real hardware) — this is necessary but
not sufficient usability evidence. It cannot substitute for formative/summative evaluation with
representative users, which remains the manufacturer's responsibility.

## 5. User interface design and implementation

> `IEC 62366-1:2015 §5.5 User interface design and implementation`

Implemented across `trustsc-ui-dsl-authoring` (the `.medui` compiler), `trustsc-ui` (UI policy/runtime
types), and `adapters/trustsc-vulkan-winit` (Vulkan rendering, glyph atlas upload, the winit event
loop).

## 6. Formative evaluation

> `IEC 62366-1:2015 §5.6 Formative evaluation`

Not conducted by TrustSC itself — a manufacturer's iterative usability evaluation activities are
theirs to run and document, using their own `.medui` screens as the artifact under test.

## 7. Summative evaluation

> `IEC 62366-1:2015 §5.7 Summative evaluation`

Not conducted by TrustSC itself, and not automatable by `--verify-ui` (see §4 above) — summative
evaluation for hazard-related use scenarios requires real representative users and is the
manufacturer's responsibility.

## Localization and text-budget policy

Relevant supporting evidence for usability engineering: every `t("key")` reference in a `.medui`
file is checked at compile time against all approved locales' text widths
([ADR-004](../../../docs/adr/ADR-004-unicode-localization-and-fallback-policy.md),
[ADR-010](../../../docs/adr/ADR-010-medui-i18n-and-text-budget-policy.md)) — a translation that
would overflow its allocated bounds in any approved locale fails the build, rather than truncating
or overlapping at runtime in a way that could confuse or mislead an operator.

## Justification records

```json
{
  "justification_id": "JUS-004",
  "standard": "IEC 62366-1",
  "clause_ref": "IEC 62366-1:2015 §5.2 Identify frequently used functions and hazard-related use scenarios",
  "rationale": "The @safety_critical MedUI annotation is the mechanism ADR-011 intends for binding a hazard-related UI element back to the compliance program via its requirement id; the .medui compiler does not yet enforce that a @safety_critical node actually carries a requirement, so today this is a documented convention a reviewer checks for, not yet a build-time guarantee.",
  "evidence_refs": [
    "docs/adr/ADR-011-medui-safety-monitor-and-vulkan-viewport-contract.md",
    "docs/dsl/safety-monitor-contract.md"
  ]
}
```
