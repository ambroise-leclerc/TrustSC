# Architecture decision records

Every ADR below is `Status: Accepted`. They are the authoritative source for *why* an
architectural boundary exists — read them before proposing a change that would cross a governed/
adapter/tools boundary, or alter a compile-time-only contract.

| ADR | Title | Summary |
|---|---|---|
| [ADR-001](ADR-001-safety-critical-text-rendering-architecture.md) | Safety-critical text rendering architecture | Full Unicode/shaping/bidi handling stays offline; the runtime only consumes pre-compiled, immutable text packages. |
| [ADR-002](ADR-002-reproducible-font-asset-pipeline.md) | Reproducible font asset pipeline | Deterministic, byte-verified glyph-atlas compilation from a vendored, provenance-tracked font asset. |
| [ADR-003](ADR-003-deterministic-runtime-text-package.md) | Deterministic runtime text package and memory model | The runtime text package is immutable and self-validating at startup — no allocation, no reparsing. |
| [ADR-004](ADR-004-unicode-localization-and-fallback-policy.md) | Unicode, localization, and fallback policy | How approved locales, fallback, and text budgets are enforced at build time. |
| [ADR-005](ADR-005-pure-rust-project-boundary-and-dependency-policy.md) | Pure-Rust project boundary and dependency policy | Defines the governed (`crates/`) vs. edge-adapter (`adapters/`) vs. host-only (`tools/`) trust zones that everything else builds on. |
| [ADR-006](ADR-006-vulkan-versus-vulkansc-profile-strategy.md) | Vulkan versus Vulkan SC profile strategy | How Class B (Vulkan) and Class C (Vulkan SC) targets share one UI policy layer. |
| [ADR-007](ADR-007-compliance-evidence-and-generated-artifact-ownership.md) | Compliance evidence and generated artifact ownership | The bake/`report.json`/CI-`verify` pattern used by every generated evidence artifact in the repo. |
| [ADR-008](ADR-008-deterministic-medui-dsl-boundary.md) | Deterministic MedUI DSL boundary | `.medui` is parsed and validated only at build time — never on-device. |
| [ADR-009](ADR-009-medui-compilation-and-generated-artifacts.md) | MedUI compilation and generated artifacts | How a `.medui` file becomes a generated `CompiledScreenPackage` Rust module. |
| [ADR-010](ADR-010-medui-i18n-and-text-budget-policy.md) | MedUI i18n and text-budget policy | Every `t("key")` reference is checked at compile time against all approved locales' widths. |
| [ADR-011](ADR-011-medui-safety-monitor-and-vulkan-viewport-contract.md) | MedUI safety-monitor and VulkanViewport contract | The `@safety_critical` annotation and the bounded realtime viewport data plane. |
| [ADR-012](ADR-012-presentation-adapter-crates-and-shader-artifacts.md) | Presentation adapter crates, the `adapters/` directory, and shader artifact evidence | Formalizes `adapters/` and the committed, byte-verified SPIR-V shader evidence pattern. |
| [ADR-013](ADR-013-host-preview-and-bounded-realtime-contract.md) | Host preview of Vulkan SC profiles and the bounded realtime contract | How a Class C application can be previewed on ordinary Vulkan hardware. |
| [ADR-014](ADR-014-precise-positioning-and-image-asset-governance.md) | Precise positioning, image asset governance, and theme colors | Pixel-exact `position:` layout, governed image assets, and theme color tokens. |
| [ADR-015](ADR-015-widget-organization-principles.md) | Widget organization principles | Compiled-retained structure, immediate-mode data plane, bounded input events. |
| [ADR-016](ADR-016-automated-ui-verification-and-manual-generation.md) | Automated UI verification and manual generation | Offscreen rendering, rendered-truth checks, and evidence reports (`--verify-ui`). |
| [ADR-017](ADR-017-zero-soup-ml-inference-pipeline.md) | Zero-SOUP machine-learning inference pipeline | Weights as baked, byte-verified data; a from-scratch, fail-closed deterministic inference engine. |
| [ADR-018](ADR-018-signal-trace-node.md) | SignalTrace node and 2D line-strip pipeline | A bounded scrolling-waveform primitive for raw physiological signals. |

See also [`docs/governance/soup-register.toml`](../governance/soup-register.toml) for the
third-party dependency register these ADRs reference, and
[`docs/regulatory-compliance.md`](../regulatory-compliance.md) for how this ADR trail fits into
an IEC 62304 technical file.
