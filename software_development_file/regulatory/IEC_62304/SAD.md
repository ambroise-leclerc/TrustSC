# Software Architecture Design (SAD) — TrustSC

> Filled-in example for TrustSC itself. See
> [`software_development_file/templates/IEC_62304/SAD.md`](../../templates/IEC_62304/SAD.md) for the
> blank template, and [`docs/iec62304/03-development-design.md`](../../../docs/iec62304/03-development-design.md)
> for the underlying clause guidance.

## Document control

- **Product / software item:** TrustSC — Rust medical-device UI/ML SDK
- **Version:** workspace `Cargo.toml` version at time of reading; see `Cargo.lock` for the exact
  resolved dependency graph
- **Safety classification:** Class B or Class C, chosen per-device by the manufacturer via
  `trustsc_core::DeviceContext.safety_class` — see `IEC 62304:2006 §4.3 Software safety classification`
- **Author(s):** TrustSC maintainers
- **Date:** see `docs/adr/README.md` for the dated ADR trail this SAD summarizes

## 1. Purpose and scope

This SAD describes TrustSC's own architecture as an SDK — the software items a manufacturer
integrates into their device software, not a finished device. It is the applied counterpart to
[`docs/iec62304/03-development-design.md`](../../../docs/iec62304/03-development-design.md).

## 2. Software items and their decomposition

> `IEC 62304:2006 §5.3.1 Transform requirements into an architecture`

Three trust zones, formalized by [ADR-005](../../../docs/adr/ADR-005-pure-rust-project-boundary-and-dependency-policy.md):

- **`crates/` — governed** (`trustsc-core`, `trustsc-governance`, `trustsc-ui`, `trustsc`, `trustsc-text-schema`,
  `trustsc-text-authoring`, `trustsc-text-runtime`, `trustsc-ml-schema`, `trustsc-ml-authoring`,
  `trustsc-ml-runtime`, `trustsc-ui-dsl-authoring`) — pure Rust, `#![forbid(unsafe_code)]`, no FFI/native
  handles in their public API.
- **`adapters/` — edge adapters** (`trustsc-vulkan-winit`) — the only crate using `unsafe`/native SDK
  bindings (`ash`, `ash-window`, `raw-window-handle`, `winit`), per
  [ADR-012](../../../docs/adr/ADR-012-presentation-adapter-crates-and-shader-artifacts.md). Every
  public function takes/returns owned governed types only — no foreign handle crosses the boundary.
- **`tools/` — host-only** (`trustsc-font-baker`, `trustsc-image-baker`, `trustsc-shader-baker`,
  `trustsc-ml-baker`) — never linked into device/runtime artifacts; tracked separately in
  `docs/governance/soup-register.toml`.

## 3. Interfaces between software items

> `IEC 62304:2006 §5.3.2 Develop an architecture for the interfaces of software items`

`trustsc-core` → `trustsc-governance` → `trustsc-ui`/`trustsc-text-*`/`trustsc-ml-*` → the `trustsc` facade →
`adapters/trustsc-vulkan-winit`. `FrameworkBuilder` (`crates/trustsc/src/lib.rs`) is the composition root:
it wires `DeviceContext` + `ComplianceProgram` + `UiSdkConfig` + `UiComponent`s together and
cross-validates them (a Class C device is rejected unless its UI config uses the Vulkan SC profile;
a UI component must reference a requirement that actually exists in the compliance program) before
producing a `Framework`. The adapter's public surface (`App::new(framework, screen)`/`.run(...)`)
only ever takes owned `Framework`/`CompiledScreenPackage` values.

## 4. Segregation for risk control

> `IEC 62304:2006 §5.3.3 Identify segregation necessary for risk control`

`unsafe` code and native SDK bindings are confined to `adapters/`; every governed crate carries
`#![forbid(unsafe_code)]` at the crate root, so this segregation is enforced by the compiler, not by
convention or code review alone — a governed crate that tried to introduce `unsafe` code or a
foreign handle in its public API would fail to compile.

## 5. SOUP identification

> `IEC 62304:2006 §5.3.4 Identify SOUP items`

See [`software_development_file/regulatory/IEC_62304/SOUP.md`](SOUP.md), derived from
[`docs/governance/soup-register.toml`](../../../docs/governance/soup-register.toml).

## 6. Architecture verification

> `IEC 62304:2006 §5.3.5 Verify the architectural design`

Every structural decision described above is recorded as an `Accepted` ADR with its context,
decision, and consequences — see [`docs/adr/README.md`](../../../docs/adr/README.md) (19 ADRs at
time of writing). A change to the architecture requires a new or superseding ADR, giving this SAD a
traceable review record rather than a document that drifts silently from the code.

## Justification records

```json
{
  "justification_id": "JUS-001",
  "standard": "IEC 62304",
  "clause_ref": "IEC 62304:2006 §5.3.3 Identify segregation necessary for risk control",
  "rationale": "unsafe code and native SDK handles are confined to adapters/trustsc-vulkan-winit; every governed crate under crates/ carries #![forbid(unsafe_code)] at the crate root, making the trust-zone segregation a compiler-enforced property rather than a reviewed convention.",
  "evidence_refs": [
    "docs/adr/ADR-005-pure-rust-project-boundary-and-dependency-policy.md",
    "docs/adr/ADR-012-presentation-adapter-crates-and-shader-artifacts.md",
    "crates/trustsc-core/src/lib.rs",
    "adapters/trustsc-vulkan-winit"
  ]
}
```
