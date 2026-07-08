# ADR-016: Automated UI verification and manual generation — offscreen rendering, rendered-truth checks, and evidence reports

## Status

Accepted

## Context

MduX verifies a great deal at compile time: containment, no-overlap and per-locale text budgets
(ADR-010, ADR-014) make an over-wide translation or a colliding component a build failure. But
nothing in the project ever verifies **rendered truth** — that the pixels a device actually
produces match what the compiled screen, the theme table and the approved text packages promise.
For a Class C UI, that gap is the difference between "the layout specification is evidence" and
"the rendering of that specification is evidence". Six capability gaps block an automated UI
testing system today:

1. **Golden references have no consumer.** Every positioned or `@safety_critical` node compiles
   to a `GoldenReferenceEntry` with `cv_checks: [Bounds | ColorHash]` (ADR-011, ADR-014), and
   `docs/dsl/safety-monitor-contract.md` promises the entries are "exposed for inspection and
   automated tests" — yet the only runtime consumer prints their count, and `ColorHash` has
   never been computed anywhere in the workspace.
2. **No offscreen rendering or pixel readback exists.** The window is load-bearing in exactly
   four places in the adapter — instance WSI extensions, surface creation, present-support
   device filtering, and the swapchain extent — and the render pass hardcodes its color
   attachment's `final_layout` to `PRESENT_SRC_KHR`. Everything else, including the entire
   command recording and every resource builder, is window-independent. There is no
   image-to-buffer copy anywhere; every existing copy goes the other way (atlas uploads).
3. **Nothing verifies actual colors, glyph presence, or per-locale rendered geometry.** The
   theme table (`THEME_COLORS`, ADR-014) defines exact RGBA values and the surface format is
   UNORM with no shader color conversion — a solid fill's expected byte is exactly
   `round(255 × token_float)` — but no check ever samples a rendered pixel against it, and the
   ADR-010 budgets prove translations *should* fit without ever rendering one.
4. **Control behavior has unit tests but no scripted, evidenced scenarios.** The ADR-015 input
   plane is deterministic and injectable (`WidgetEvent` → `FrameEvents` → application closure →
   `FrameInputs` echo), yet interaction flows ("type a patient id, acknowledge the alert") are
   demonstrated by hand on a bench, leaving no machine-checkable trace.
5. **Compliance exports are pipe-delimited text.** `trace_matrix_export`, `audit_export` and
   `release_summary` serve human inspection; no machine-readable artifact ties a requirement and
   its verification cases to concrete rendered evidence (a screenshot digest, measured bounds,
   sampled colors, a scenario trace).
6. **No locale enumeration and no manual pipeline.** `TextPackage` cannot list its locales, so
   nothing can loop "verify every supported translation"; and user-manual screenshots would
   today be hand-taken, unlocalized, and stale the moment a screen changes.

A note on practice: mainstream screenshot testing divides into pixel-exact golden images and
property-based verification. Pixel-exact goldens are simple but fragile across GPU rasterizers —
glyph antialiasing differs legitimately between implementations — while property checks
(sampled solid colors, geometry containment, ink coverage) are backend-independent but cannot
pin every pixel. This ADR uses both, with an honest division of labor: properties are the
certified gate, exact hashes are backend-scoped regression evidence.

## Decision

### 1. Offscreen render path — the same pixels, without a window

`adapters/mdux-vulkan-winit` gains an `OffscreenRenderer` (new `src/offscreen.rs`):

- A **headless instance** skips `ash_window::enumerate_required_extensions` (keeping the macOS
  portability-enumeration bits) and a headless device pick drops the present-support filter,
  keeping the graphics-queue requirement. `create_render_pass` gains a `final_layout` parameter:
  `PRESENT_SRC_KHR` windowed, `TRANSFER_SRC_OPTIMAL` offscreen.
- Rendering targets one `R8G8B8A8_UNORM` image (`COLOR_ATTACHMENT | TRANSFER_SRC`) at the
  **authored surface extent**, so every measured coordinate equals the compiled and golden
  coordinates 1:1 — no DPI, no scaling, no coordinate mapping in the verification path.
- `draw_frame(inputs, clock, interaction)` reuses the **same `record_command_buffer` and
  resource builders as presented frames** — verification honesty by construction: there is no
  parallel "test renderer" to drift from the product. `read_pixels()` performs a one-shot
  image-to-buffer copy and returns tightly packed RGBA (`CapturedFrame { width, height, rgba }`).
- Determinism: `WallClock` is already an injected parameter; verification pins a fixed clock so
  `Clock` nodes render identical glyphs in every run. Same screen + locale + inputs + backend ⇒
  identical bytes.

All `unsafe`/ash stays in the adapter (ADR-005 intact).

### 2. `mdux-ui-verify` — the pure check engine, first consumer of golden references

A new governed crate, `crates/mdux-ui-verify`: pure Rust, `#![forbid(unsafe_code)]`, **zero
dependencies**. It takes `FramePixels { width, height, rgba }` plus the compiled screen (and
per-locale expectations resolved by the caller) and returns typed `CheckResult`s. Being
dependency-free keeps it fully unit-testable on synthetic pixel buffers without a GPU, and
reusable by the external safety monitors the safety-monitor contract anticipates. The check
vocabulary:

- **GoldenBounds** — for every golden reference with `Bounds`: the node's ink bounding box
  (pixels differing from the sampled surrounding background) lies entirely inside
  `entry.bounds`.
- **ChromeColor** — for every node with a resolvable color token and a glyph-free samplable
  region (panel interiors, button-face bands inset from edges and label, text-input field
  bands, status regions): every sampled pixel equals `round(255 × THEME_COLORS[token])` within
  a per-channel tolerance of **ε = 1** (UNORM rounding). Sampling regions derive from the
  compiled node kind, never from guesses.
- **TextPresence** — for every statically texted node and every scenario-known dynamic text:
  the ink coverage ratio inside the text bounds falls within a band derived from the active
  locale's compiled run glyph count. This catches blank, clipped or garbled text without
  comparing antialiased pixels byte-wise.
- **InkContainment** — zero ink pixels outside each node's bounds, checked per node per locale.
  Because compiled bounds are statically disjoint (ADR-014), simultaneous containment of every
  node's ink proves **no rendered overflow and no rendered overlap in every supported
  translation** — the rendered-truth counterpart of the compile-time budget rules.
- **ColorHash** — defined for the first time (§3).

The crate also owns `emit_report_json(&VerificationReport) -> String`: a hand-rolled,
deterministic JSON emitter (fixed key order, integers only — coverage as parts-per-million),
so reports are byte-reproducible and no serde enters governed or adapter code.

### 3. ColorHash — two honest tiers

> `ColorHash(entry) = sha256(RGBA8 bytes of the rect entry.bounds, row-major, top-to-bottom,
> tightly packed, rendered at the authored surface extent on R8G8B8A8_UNORM)`

- **Tier 1 — the certified gate is backend-independent.** GoldenBounds, ChromeColor,
  TextPresence and InkContainment pass identically on every conformant implementation, because
  no Tier-1 check compares antialiased pixels byte-wise.
- **Tier 2 — ColorHash is exact and backend-scoped.** Glyph antialiasing legitimately differs
  across rasterizers, so hashes are pinned per `backend_id` — a normalized device name. The
  committed baselines are **lavapipe only** (Mesa's Vulkan software rasterizer, installed in
  CI): CI byte-compares them on every run and any mismatch fails. On any other backend the
  check reports `no_baseline` — informational, and **never a pass**. Naming note: lavapipe
  reports its Vulkan device name as `llvmpipe (LLVM …)` after its rasterization core; the
  `backend_id` normalization maps that device name to `lavapipe`, and every baseline path, CI
  step and document uses `lavapipe` exclusively.
- No quantization: quantized hashes flip on bucket edges without giving real tolerance; the
  honest statement is that this hash is exact regression evidence for one backend.

### 4. Scenario scripts — authored TOML, compiled to static data

Control behavior is verified by **scenario scripts**: authored TOML under
`examples/<app>/verify/scenarios/*.toml`, compiled at build time by `mdux-build` into static
Rust data (`ScenarioScript { id, requirement_ids, clock, steps }`) — the ADR-008 doctrine
applied again: authored source in, static data out, no runtime parsing. Steps are: inject a
`WidgetEvent`; `expect_text` / `expect_status` / `expect_number` against `FrameInputs`;
`capture` a named offscreen frame. The runner (in the `mdux` facade) replays events through
`FrameEvents` into the application's registered `with_input` and `with_realtime` closures and
asserts the echoed state — this half is GPU-free and doubles as plain `cargo test` coverage.
`capture` steps render offscreen with an `InteractionSnapshot` synthesized from the scripted
focus/press state and run the full check suite on the captured frame. Every step's
event → expected → observed trace lands in the report.

### 5. Evidence reports — a new ADR-007 artifact class

One report per `(application, locale)` under `generated/verification/<app>/<locale>/`:
`report.json`, `screenshot.ppm`, and `step-<scenario>-<label>.ppm` captures; committed lavapipe
baselines under `generated/verification/<app>/baselines/lavapipe/`. The report (schema v1)
records: tool name/version, device identity and safety class, screen id, locale, surface
extent, backend id and device name, the pinned clock, the screenshot digest, every check with
its expected and measured values (RGBA and max channel delta, ink bounds, coverage ppm,
hashes), every scenario step trace, and a **trace join** — requirement id → verification ids →
check ids/scenario ids — built from `CompiledNodeKind::requirement_id()` and a new structured
`ComplianceProgram::trace_rows()` accessor. This is the artifact a manufacturer attaches to a
VER-xxx test or demonstration case. Verification runs exit non-zero on any failed check, so the
same artifact is the CI gate.

Because screens are `&'static` data inside application binaries, verification is an **App run
mode**, symmetric with `--headless-smoke`: `--verify-ui=<dir>`, `--locales=all|<list>`,
`--scenario=<id>`.

### 6. Locale enumeration

`TextPackage` gains `locales()` — the sorted, deduplicated union of every font's declared
locales — plus a new validation rule: every approved string's and compiled run's locale must be
declared by a font. `--locales=all` loops verification over this list, so "no overflow or
overlap in any supported translation" is checked against rendered frames for **every** locale
the package carries, automatically including locales added later.

### 7. Manual generation — captures in-app, prose in a host tool

- **Stage 1, in-app** (`--manual-capture=<dir>`): reusing the verification machinery, render
  the base screen and every scenario capture per locale at the authored extent; emit PPMs plus
  `capture-index.json` (screen id, locale, node table with ids/bounds/resolved per-locale label
  text, capture list). Callout text is resolved from the approved strings — the manual can
  never disagree with what the device renders.
- **Stage 2, host tool** `tools/mdux-manual-gen` (`generate <manual.toml> <capture-dir>
  <out-dir>`, the house CLI shape): composites numbered callout markers at node bounds,
  encodes PNG with a **hand-rolled stored-deflate PNG encoder** (~200 lines — zero new SOUP
  dependencies, the PPM-parser precedent applied to output), and emits
  `generated/manual/<app>/<locale>/manual.md` + `images/*.png`.
- Per-application `manual.toml` holds localized titles, headings and body prose plus the
  section → capture/callout mapping. This prose is **host documentation, deliberately outside
  the approved-string boundary**: it is never rendered by the device, so forcing it through the
  baked-text pipeline would burden the evidence chain without adding device-safety value. The
  boundary stays: everything the device renders is approved and baked; everything the manual
  *says about it* is documentation.

### 8. CI — lavapipe as the reference software rasterizer

The workflow installs `mesa-vulkan-drivers` (lavapipe; surfaceless, no display server), runs
`--verify-ui --locales=all` for every example, byte-compares the committed lavapipe ColorHash
baselines, regenerates the committed manual and byte-compares it (the evidence doctrine applied
to documentation), and uploads `generated/verification/` as build artifacts so every CI run
leaves inspectable regulatory evidence.

## Consequences

- Golden references finally do what ADR-011 promised: every entry is machine-verified against
  rendered pixels on every CI run, and `ColorHash` gains its first precise definition.
- Verification reuses the exact production render path — there is no second renderer to drift —
  at the cost of the adapter growing an offscreen module and the render pass one parameter.
- The exact-pixel tier is honestly backend-scoped: lavapipe baselines are enforced, developer
  GPUs report informationally. The certified gate never depends on antialiasing agreement.
- Every supported translation now produces committed, rendered evidence per release; adding a
  locale to the text package automatically adds it to verification and to the manual.
- Scenario TOML becomes a second authored-source class compiled like `.medui`; behavior
  evidence (event → expected → observed) attaches to requirements instead of living in bench
  notes.
- Two hand-rolled components (JSON emitter, PNG encoder) are accepted as the price of keeping
  serde and image libraries out of governed code and the SOUP register unchanged.
- The `generated/verification/` and `generated/manual/` trees join `generated/fonts` and
  `generated/images` as ADR-007 evidence outputs: regenerated by tooling, never hand-edited.

## References

- ADR-005 (dependency boundary) — offscreen Vulkan stays in the adapter; hand-rolled
  encoders keep SOUP unchanged
- ADR-007 (evidence ownership) — verification reports and manuals are new generated artifact
  classes
- ADR-008 (deterministic DSL boundary) — scenario TOML compiles to static data the same way
- ADR-010 (i18n budgets) — the compile-time promise these checks verify against rendered frames
- ADR-011 (safety monitor contract) — golden references gain their first consumer
- ADR-013 (bounded realtime, injected WallClock) — the determinism seams verification relies on
- ADR-014 (theme table, precise positioning) — the exact color and geometry expectations
- ADR-015 (input plane) — the injectable event vocabulary scenarios replay
- docs/dsl/safety-monitor-contract.md — the promise this ADR fulfils
- Epic #91
