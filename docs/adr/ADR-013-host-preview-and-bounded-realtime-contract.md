# ADR-013: Host preview of Vulkan SC profiles and the bounded realtime contract

## Status

Accepted

## Context

The next demonstration application is a Class C realtime monitor (epic #30): a top bar with
date/time and a status indicator, large numerals showing a live sedation index, and a 3D spectral
waterfall fed by streaming data. Two structural gaps block it.

First, a Class C application cannot open a window today. `FrameworkBuilder::build` enforces
`SafetyClass::C ⇒ GraphicsProfile::VulkanSc` (an ADR-006 rule the project does not want to relax),
while the only presentation adapter, `adapters/trustsc-vulkan-winit`, drives standard Vulkan 1.x and
never inspects the framework's profile. Real Vulkan SC drivers do not exist on developer
workstations, yet a Class C UI still has to be *seen* during development. Running SC-profile
content on the standard adapter must therefore become an explicit, documented, audited
affordance — not a silent accident of a missing check.

Second, nothing in the system updates after startup. The renderer records its command buffers
once at swapchain creation, uploads a single static vertex buffer, and receives no application
data per frame. Realtime widgets (clock, live numerals, status, streaming 3D) need a per-frame
update path — and for a Class C target whose `DeterminismPolicy::vulkan_sc` forbids runtime
allocation and runtime object creation, that path must be designed bounded from the start, not
retrofitted.

## Decision

1. **`adapters/trustsc-vulkan-winit` MAY run a framework whose `graphics_profile` is `VulkanSc`, as
   a host development preview only.** The governed validation chain is not relaxed anywhere:
   Class C still requires the `VulkanSc` profile, offline-compiled pipelines, the
   zero-runtime-allocation determinism policy, non-zero reserved budgets, and at least one
   hazard. When the adapter receives such a framework it must, before opening any window and on
   the headless path alike:
   - print, among the startup diagnostics, the exact banner line
     `profile=Vulkan SC (HOST PREVIEW on standard Vulkan — not the certified pipeline)`;
   - record a `Runtime`-category audit event with the exact message
     `vulkan sc host preview: rendering on standard Vulkan for development only` on the
     framework's compliance trail, so every preview execution is visible in the exported audit
     log. To allow this, the `trustsc` facade gains
     `Framework::record_runtime_event(&mut self, message)` — the only post-`build()` mutation the
     facade exposes, restricted to the `Runtime` audit category.
   A framework with the `Vulkan` profile behaves exactly as before; no banner, no event.

2. **The realtime data path follows a bounded contract.** All capacities are fixed when the
   renderer is constructed and derived from the compiled screen: the maximum number of dynamic
   glyph quads is the sum of the screen's realtime bindings' capacities, and each streaming
   viewport declares a fixed ring buffer of `rows × bins` samples. Per-frame work is limited to
   (a) rewriting persistently mapped, pre-allocated buffers and (b) re-recording command buffers
   from pre-allocated pools. **No heap allocation and no Vulkan object creation may occur per
   frame.** The host preview thereby exercises the same discipline `DeterminismPolicy::vulkan_sc`
   will demand of a future true Vulkan SC adapter, keeping the preview honest as a rehearsal of
   the certified path rather than a divergent branch.

3. **The monitor widget set.** MedUI gains four widgets and one container, specified here so the
   model (`trustsc-ui`), the compiler (`trustsc-ui-dsl-authoring`), the facade bindings and the adapter
   implement one vocabulary:
   - `Row { … }` — a horizontal container nested exactly one level inside the screen's vertical
     layout. It exists **at compile time only**: the DSL compiler resolves its children to
     absolute bounds and the emitted `CompiledScreenPackage` stays flat and static (ADR-008
     intact).
   - `Label` — static approved text (`t("key")`), decorative: it carries no requirement and
     derives no UI component.
   - `Clock` — wall-clock date/time rendered from the numeric glyph path (digits, `:`, `-`,
     space); its content comes from the platform clock via the adapter, so applications write
     zero code for it. Formats: `TimeSeconds` (`HH:MM:SS`) and `DateTimeSeconds`
     (`YYYY-MM-DD HH:MM:SS`).
   - `NumericDisplay` — a live number bound to a `NumericTemplate` and a named data source;
     requirement-bearing, eligible for `@safety_critical`.
   - `StatusIndicator` — an enumerated state display; each state is an approved string with a
     color token; requirement-bearing, eligible for `@safety_critical`.
   Digits-only rendering makes `NumericTemplate`'s prefix and suffix runs **optional** in the
   text schema (whitespace approved strings are structurally invalid, so optionality is the only
   sound representation), and the text runtime gains bounded `HH:MM:SS` / `YYYY-MM-DD`
   formatters over numeric glyph sets.

4. **Two-package font strategy.** The text schema keys compiled runs by a unique
   `(source_string_id, locale)` pair and associates no pixel size with a run, and the font baker
   produces exactly one pixel height per recipe. Rather than complicate either, display-size
   digits ship as a **second approved package**: `generated/fonts/roboto-display-48px/` baked
   from its own recipe, with `48` added to the Roboto manifest's
   `intended_baseline_pixel_heights` (same TTF, same digest — a one-line reviewed approval). The
   standard 16 px package keeps serving labels, status states and the clock. Both packages are
   ADR-007 generated evidence, both verified byte-for-byte in CI, and the facade exposes them as
   `default_standard_text_package()` and `default_display_text_package()`.

## Consequences

- Class C applications become demonstrable on developer hardware without weakening a single
  governed validation, and every such execution is self-documenting in the audit trail. The
  difference between "preview" and "certified" is a printed, logged, greppable fact.
- The bounded realtime contract gives the future Vulkan SC adapter (ADR-012's anticipated
  sibling) an already-exercised behavioral specification: same bindings, same fixed capacities,
  same no-allocation frame loop — only the pipeline provenance changes.
- The schema change (optional affixes) is the first modification to the compiled text-package
  format since ADR-003; the font-baker document format keeps emitting affix fields when present,
  so previously committed packages remain byte-identical and `verify` keeps passing.
- A second font package roughly doubles committed font evidence (~an atlas of ten 48 px digits);
  the cost is accepted in exchange for keeping the run-keying and baker models untouched.
