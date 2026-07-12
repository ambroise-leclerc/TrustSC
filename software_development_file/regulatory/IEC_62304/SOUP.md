# SOUP list and justification — TrustSC

> Filled-in example for TrustSC itself. See
> [`software_development_file/templates/IEC_62304/SOUP.md`](../../templates/IEC_62304/SOUP.md) for
> the blank template. This document summarizes and links to the authoritative machine-readable
> register — it does not duplicate it — at
> [`docs/governance/soup-register.toml`](../../../docs/governance/soup-register.toml).

## Document control

- **Product / software item:** TrustSC
- **Register scope:** `host-tooling-and-presentation-adapters` (per the register's own `scope` field)
- **Owner:** TrustSC text authoring maintainers (per the register's `owner` field)

## 1. Purpose

> `IEC 62304:2006 §5.3.4 Identify SOUP items` / `§8.1.3 SOUP identification` / `§8.3.2 SOUP anomaly list`

`docs/governance/soup-register.toml` is the single source of truth for every third-party (SOUP)
dependency used by this project's host tooling and presentation adapter. This document is a guided
summary of it, not a second copy.

## 2. SOUP register summary

10 entries at time of writing, split by whether they are ever deployed to a device (`runtime_deployment`):

**Host-only tooling and authoring (`runtime_deployment = false`)** — never present in a built
device artifact: `fontdue` (glyph rasterization, `tools/trustsc-font-baker`), `serde` + `serde_json`
(bake-recipe and evidence `package.json`/`report.json` I/O across `trustsc-font-baker`,
`trustsc-build`, `trustsc-ml-baker`, `trustsc-shader-baker`), `sha2` (evidence digests), `toml` (manifest and
recipe parsing), `shaderc` (GLSL→SPIR-V compilation in `tools/trustsc-shader-baker`).

**Presentation adapter (`runtime_deployment = true`)** — deployed as part of
`adapters/trustsc-vulkan-winit` only: `ash` + `ash-window` (Vulkan bindings), `raw-window-handle`
(windowing-handle trait), `winit` (window/event-loop). Every entry's `boundary_rationale` states the
same governed/adapter confinement: "no `<crate>` type crosses into a governed crate's public API."

## 3. Known anomalies (§8.3.2)

Not yet tracked as a distinct list separate from each entry's `risk_controls` field — flagged as a
gap in [`docs/iec62304/07-configuration-management-process.md §8.3.2`](../../../docs/iec62304/07-configuration-management-process.md#832-soup-anomaly-list).
A manufacturer shipping a device built on TrustSC should maintain their own SOUP-anomaly tracking
(e.g. CVE monitoring for `ash`/`winit`) alongside this register rather than assuming its absence here
means none exist upstream.

## 4. SOUP update policy

Every entry's version is pinned by `Cargo.lock` and the listed `pinned_by` `Cargo.toml` path(s); CI
builds `--locked` (`.github/workflows/ci.yml`), so a dependency update requires an explicit,
reviewed `Cargo.lock` change rather than floating to a new minor/patch version silently.

## Justification records

```json
{
  "justification_id": "JUS-003",
  "standard": "IEC 62304",
  "clause_ref": "IEC 62304:2006 §5.3.4 Identify SOUP items",
  "rationale": "Every SOUP dependency is recorded once in docs/governance/soup-register.toml with supplier, license, integration_path, pinned_by, and risk_controls, and CI's --locked build makes an unreviewed version drift fail rather than merge silently.",
  "evidence_refs": [
    "docs/governance/soup-register.toml",
    ".github/workflows/ci.yml"
  ]
}
```
