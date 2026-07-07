# MedUI DSL component dictionary

## `CriticalButton`

Required properties:

- `id`
- `requirement`
- `width`
- `height`
- `label`
- `color`
- `on_press`

Notes:

- `requirement` is a MduX-rust traceability extension used to keep the generated UI aligned with the existing governance path.
- `label` must be `t("key")`.
- `on_press` is currently limited to predefined `SystemEvent` values.

## `VulkanViewport`

Required properties:

- `id`
- `width`
- `height`
- `stream_source`

Notes:

- the generated package reserves a region for direct imaging output
- it does not embed arbitrary render logic in the UI layer

## `Row`

A compile-time-only horizontal container, nested exactly one level inside a `Vertical` screen
layout. It disappears from the compiled package: its children are emitted as flat nodes with
absolute bounds (ADR-008 intact).

Required properties: `id`, `height`. Optional: `spacing` (defaults to `0px`), `background`
(a theme color token — emits a synthetic `Panel` node with id `{row_id}-background` spanning
the content width beneath the row's children; Panels carry no requirement or text and are
exempt from the overlap rule).

Notes:

- children are regular leaf components, one property per line; nesting another `Row` is rejected
- `@safety_critical` cannot annotate a `Row` itself (annotate its children)
- children with `height: Fill` take the Row's height; a child taller than the Row is rejected

## `Label`

Static approved text with no interaction and no requirement (titles, units).

Required properties: `id`, `width`, `height`, `text` (`t("key")`), `color`.

Notes:

- budgeted at compile time against **every** approved locale of the key, like button labels

## `Clock`

Wall-clock date/time fed by the platform adapter — applications write zero code for it.

Required properties: `id`, `width`, `height`, `format` (`TimeSeconds` | `DateTimeSeconds`).

Notes:

- renders from the standard package's `SET-ASCII-DIGITS` glyph set (digits, `-`, `:`, space)
- budgeted at compile time against its fixed glyph sequence (`HH:MM:SS`, or
  `YYYY-MM-DD HH:MM:SS` for `DateTimeSeconds`)
- carries no requirement and no approved text key (dynamic, platform-fed content)

## `NumericDisplay`

A live numeric value bound to an approved `NumericTemplate` and a named realtime data source.
Requirement-bearing; eligible for `@safety_critical`.

Required properties: `id`, `width`, `height`, `requirement`, `template` (quoted template id in
the **display** package), `source` (quoted realtime source name), `color`.

Notes:

- budgeted at compile time as `max_chars ×` the widest digit advance of the template's glyph
  set, plus any affix runs — the compiler therefore requires the display text package
- the golden reference emitted by `@safety_critical` pins bounds and color; the digits vary at
  runtime by design (`text_key: None`)

## `StatusIndicator`

An enumerated device-state display; the application selects the active state by index at runtime.
Requirement-bearing; eligible for `@safety_critical`.

Required properties: `id`, `width`, `height`, `requirement`, `source`,
`states` (`[t("KEY-A"), t("KEY-B"), …]`). Optional: `colors` (`[token, …]`, same length as
`states`; defaults to the neutral status token for every state).

Notes:

- **every** state label in **every** approved locale must fit the node's bounds — the widest
  translation of the widest state defines the compile-time budget

## `Image`

A governed raster image (ADR-014): the `img("IMAGE-ID")` reference must name a baked image
package (`generated/images/`), and the declared `width`/`height` must equal the package's
intrinsic dimensions **exactly** — images render at intrinsic size only, there is no runtime
scaling.

Required properties: `id`, `width`, `height`, `source: img("IMAGE-ID")`.

Notes:

- `@safety_critical` accepts `Bounds` only; `ColorHash` over image content is rejected
- typically combined with `position:` (e.g. a brand mark pinned to the top bar)

## `Button`

The application-semantic interactive button (ADR-015). A press is delivered to the application
as a `ButtonPressed { source }` event through the bounded outbound event plane — by data, not by
callback. What a press *means* belongs to the application; framework-governed system events
belong to `CriticalButton`, so declaring `on_press` on a `Button` is a compile error.

Required properties: `id`, `width`, `height`, `label` (`t("key")`), `color`,
`source` (quoted event key, e.g. `"ACK_BUTTON"`). Optional: `requirement` (traced when present).

Notes:

- the label is static approved text, budgeted at compile time against **every** approved locale
- the face and pressed tints are derived from the `color` token at binding time, never per frame
- eligible for `@safety_critical`; its golden reference pins label key, color and bounds

## `TextInput`

An operator-editable text field (ADR-015): a **controlled component** — the application owns the
buffer, applies the editing events it drains each frame, and echoes the result back through
`FrameInputs::set_text`; the renderer stores nothing. Content is restricted to a baked, approved
charset and a declared maximum length.

Required properties: `id`, `width`, `height`, `source` (quoted echo/event key), `max_length`
(positive integer, a character count). Optional: `charset` (named approved charset; defaults to
`AsciiText` → the standard package's printable-ASCII `SET-ASCII-TEXT` glyph set), `color`
(required), `requirement` (traced when present).

Notes:

- budgeted at compile time as `max_length ×` the widest glyph advance of the declared charset —
  an over-budget `max_length` **fails the compile**, mirroring the `NumericDisplay` fit check
- `on_press` is rejected, like on `Button`
- the golden reference pins bounds and color; the typed content varies at runtime by design
  (`text_key: None`, the `NumericDisplay` precedent)
- the charset boundary is re-enforced at runtime: `set_text` rejects characters outside the
  baked glyph set and content beyond `max_length` with typed errors

## Precise positioning (`position:`, any leaf component)

Any leaf component may declare `position: <X>px, <Y>px;` — the **absolute screen coordinates**
of its top-left corner (ADR-014). A positioned component is out of flow: `Fill` siblings
distribute space as if it did not exist, and `Fill` is rejected on the positioned component
itself. The compiler verifies, at build time:

1. **containment** — inside the declaring `Row`'s bounds (row children) or the padded content
   box (top-level);
2. **no overlap** — against every other node, flow or positioned (Panels exempt);
3. **text budgets** — the widest approved translation must still fit the pinned box, so an
   internationalization growth *fails the compile*;
4. **golden evidence** — every positioned node automatically receives a `Bounds` golden
   reference (merged with `@safety_critical`'s entry when both apply).

The optional screen-level `surface: <W>px, <H>px;` declaration (right after `layout:`) pins the
authored surface against the build configuration, and the generated module always exports
`GENERATED_MEDUI_SURFACE` so the application configures `UiSdkConfig` from the compiled truth.
