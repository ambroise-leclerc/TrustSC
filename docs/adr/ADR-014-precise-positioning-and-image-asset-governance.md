# ADR-014: Precise positioning, image asset governance, and theme colors

## Status

Accepted

## Context

The NeuroSense 500 monitor (epic #30) is laid out entirely by flow: the vertical cursor and the
`Row` container compute every component's rectangle, and authors only control sizes. That is not
enough for a regulated device UI. A design specification frequently prescribes *exact* pixel
positions — "the application logo sits at (1768, 8), 144×48" — and those positions must be
**verified by the engine and reproducible as evidence**, not hoped for. The sharpest requirement
is the failure mode: when an element's content grows for a reason invisible to the layout author
— typically a wider translation arriving through the internationalization pipeline — a precise
positioning specification must **trigger an alert** rather than silently shift or clip pixels on
a certified screen.

Three capability gaps block this today:

1. The DSL has no notion of position. `parse_component_properties` recognizes several
   properties (id, sizes, text keys, colors, and more depending on component kind), but none of
   them is a coordinate — every component's rectangle is entirely a function of the flow layout.
   The compiler's containment checks exist but there is nothing to check a *declared* position
   against.
2. There is no governed asset class for images. The only vendored asset is the Roboto font,
   whose ADR-002/003 pipeline (manifest with sha256/license/provenance, deterministic baker,
   committed evidence, CI verification) has no image equivalent — yet a logo is exactly the kind
   of content a manufacturer must trace.
3. Color tokens are never resolved. `Theme.Colors.*` strings survive into compiled specs and
   golden references, but no table maps them to actual RGBA values, so nothing can render a
   "light gray background" and token typos are undetectable.

A fourth, adjacent gap surfaced while sizing the target screen: large numerals at 120 pt
(= 160 px at the 96 DPI reference of this project) need a second display font package, and
`TextPackages.display` is a singleton by construction.

## Decision

### 1. `position:` — absolute, out-of-flow, engine-verified placement

A leaf component MAY declare `position: <X>px, <Y>px;` — the absolute screen coordinates of its
top-left corner. Semantics:

- A positioned node is **out of flow**: it does not advance the flow cursor, and `Fill` siblings
  distribute space as if it did not exist. `Fill` is incompatible with `position` (a positioned
  node must have fixed `width`/`height`); the combination is a compile error.
- **Verification rule 1 — containment.** The positioned rectangle must lie entirely inside its
  *declaring container*: the resolved bounds of its `Row` for a row child, the padded content
  box for a top-level node. Escaping is a compile error naming both rectangles.
- **Verification rule 2 — no overlap.** After layout, every positioned node is tested for strict
  AABB intersection (shared edges are legal) against **every** other node, flow or positioned.
  `Panel` nodes are exempt — backgrounds are underlays by definition. The first violation in
  document order is a compile error naming both nodes. Flow-vs-flow overlap remains impossible
  by construction, so the pass is O(positioned × nodes).
- **Verification rule 3 — content budgets.** Positioned nodes go through the unchanged ADR-010
  text-budget validation: if the widest approved translation of any referenced string no longer
  fits the pinned box, **the compile fails**. This is the "internationalization triggers an
  alert" guarantee, and it fires at build time — before the device ever ships.
- **Verification rule 4 — positioning spec = golden evidence.** Every node with an explicit
  `position` automatically receives a `GoldenReferenceEntry` with `cv_checks: [Bounds]`, even
  without `@safety_critical`. A declared position is a safety-relevant claim; it becomes
  reproducible, machine-checkable evidence in the compiled package. When the node also carries
  `@safety_critical`, the compiler emits **one merged entry** (deduplicated union of cv_checks),
  never two entries for one node id.

### 2. `surface:` — declared-vs-configured redundancy

A screen MAY declare `surface: <W>px, <H>px;` immediately after its `layout:` line. If present
and different from the surface configured by the build (`CompileOptions`), the compile fails.
This is a deliberate redundancy: the `.medui` file states the surface its positions were designed
for, and the build cannot silently compile it for another one. The generated module always
exports `pub const GENERATED_MEDUI_SURFACE: (u32, u32)` so the application feeds `UiSdkConfig`
from the compiled truth instead of repeating literals.

### 3. Zero padding and full-bleed backgrounds — no special case

`0px` becomes legal for layout `spacing`/`padding` and for `position` coordinates (component
`width`/`height` still reject zero). A `Row` MAY declare `background: <token>;`, which emits a
synthetic `CompiledNodeKind::Panel` node (id `{row_id}-background`, bounds = the content-box
width at the row's y and height) *before* the row's children. There is **no full-bleed
exception** to the containment rule: a screen that wants a genuinely edge-to-edge top bar
declares `padding: 0px`, and the ordinary containment invariant holds everywhere. Panels carry
no requirement and no text, and are exempt from the overlap rule.

### 4. Governed theme colors

`trustsc-ui` owns `THEME_COLORS: &[(&str, [f32; 4])]`, the single approved token → RGBA table
(`Theme.Colors.TopbarBackground` light gray, `Title`, `ScoreDigits`, `Nominal`, `Alert`,
`Fault`, `Neutral`, `PrimaryAction`), with `resolve_color_token()`. The compiler validates every
color-bearing property (`color`, `colors`, `background`) against the table; an unknown token is
a compile error listing the approved tokens. The adapter resolves Panel colors through the same
table at binding time. **Text tint resolution is explicitly deferred**: glyphs keep the single
hardcoded overlay color for now, because per-node text color requires splitting the contiguous
dynamic-text draw ranges of ADR-013 into per-binding draws — a mechanical but separate step that
this table makes possible later without new policy.

### 5. Image assets — the ADR-002/003 pattern, applied to pixels

Images become the second governed asset class, mirroring fonts end to end:

- **Vendored source** under `assets/images/<asset-id>/`: the image file plus an
  `image-manifest.toml` (`manifest_kind = "trustsc-image-asset"`, dimensions, `source_sha256`,
  license, provenance). The source format is **binary PPM (P6)** — parsed by ~30 lines of
  hand-rolled code in the host tool, so no image-decoding dependency enters the ADR-005
  dependency budget.
- **Host baker** `tools/trustsc-image-baker` with the same `bake`/`verify` CLI contract as the font
  baker: PPM → raw RGBA8, dimensions cross-checked against the manifest, deterministic
  `package.json` + `report.json` evidence committed under `generated/images/<asset-id>/`,
  re-bake-and-byte-compare verification in CI. The first asset (the Acme placeholder logo) is
  produced by a deterministic generator function inside the baker itself, with a self-verifying
  test asserting sha256 equality against the vendored file.
- **Schema** in a new governed crate `crates/trustsc-image-schema` (`#![forbid(unsafe_code)]`,
  depends on trustsc-core only): `ImagePackage { id, width, height, pixels /* RGBA8 */, evidence }`
  with full validation. It is deliberately not in `trustsc-ui` (a const-constructible `'static`
  screen model must not carry pixel payloads) nor in `trustsc-text-schema` (wrong domain).
- **`Image` widget**: `Image { id; width; height; position; source: img("IMAGE-ID"); }`. The
  declared size must equal the baked image's intrinsic dimensions **exactly** — there is no
  runtime scaling, for the same determinism reasons text has no runtime shaping.
  `@safety_critical` on an Image accepts `Bounds` only; `ColorHash` over image content is a
  possible future check, not silently accepted today.

### 6. Display font package plurality

`TextPackages.display` generalizes from `Option<&TextPackage>` to a list. A `NumericDisplay`
template is resolved across all display packages with **unique-match** semantics: zero matches
is "unknown template" and two or more is "ambiguous across display packages" — both compile
errors, so resolution stays deterministic. The runtime bindings gain a `display_index`, and the
adapter generalizes its single display atlas to one atlas + descriptor set per package with a
fixed `[standard | d0 | d1 | …]` split of the persistently mapped dynamic buffer, offsets
computed once at startup — unchanged ADR-013 bounded contract. The 48 px package keeps its
identity and evidence; 160 px (= 120 pt) is purely additive.

## Consequences

- A `.medui` file can now *be* the pixel-exact layout specification, and the compiler is the
  reviewer: escapes, collisions, over-budget translations and surface mismatches are build
  failures with named nodes, not visual defects found on a bench.
- Every declared position is pinned in the golden references, extending the ADR-011 safety
  monitor contract to layout-by-specification.
- The image pipeline adds one governed crate, one host tool, one vendored asset directory, one
  evidence directory and one CI verify step — all shaped identically to the font pipeline, so
  auditors review one pattern, twice.
- The theme table finally makes color tokens fail loudly at compile time; runtime text tinting
  remains a documented follow-up.
- `hello_world` and existing screens compile unchanged: no `position`, no `background`, no
  `Image`, singleton display packages via compatibility constructors.

## References

- ADR-002 (reproducible font asset pipeline), ADR-003 (deterministic runtime text package) —
  the pattern images mirror
- ADR-005 (dependency policy) — why PPM instead of PNG
- ADR-008 (deterministic MedUI DSL boundary), ADR-009 (compilation artifacts)
- ADR-010 (i18n and text budget policy) — verification rule 3 builds on it
- ADR-011 (safety monitor contract) — golden references
- ADR-013 (bounded realtime contract) — the adapter constraints all rendering additions obey
- Epic #53
