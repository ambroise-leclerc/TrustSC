# ADR-015: Widget organization principles — compiled-retained structure, immediate-mode data plane, bounded input events

## Status

Accepted

## Context

The NeuroSense 500 monitor needs its first operator interactions: an acknowledge button and a
bounded patient-identifier entry field, demonstrated in `examples/class_c_monitor`. `Button` and
`TextInput` are therefore the first two input widgets the framework must carry. But interactive
widgets are where UI frameworks historically accumulate hidden state, runtime trees and
allocation churn — precisely the properties a Class C software item cannot tolerate. Before any
implementation, the project needs a stated doctrine for how widgets are organized, so that the
widget set grows composable and extensible *by design* rather than by accretion.

Three capability gaps block interactive widgets today:

1. **No input path exists at all.** The adapter event loop handles exactly `CloseRequested`,
   `Resized` (a no-op), `RedrawRequested` and `AboutToWait`; no mouse or keyboard event is ever
   inspected. `SystemEvent` (`NoOp`, `TriggerHalt`) is parsed by the DSL compiler and carried in
   every compiled `CriticalButtonSpec`, but nothing dispatches it — the realtime binding pass
   explicitly skips `CriticalButton` as "static text with no realtime state", and `TriggerHalt`
   has zero consumers. Data flows one way only: application → `FrameInputs` → renderer. There is
   no channel in the opposite direction.
2. **No stated doctrine for widget growth.** Eight node kinds exist (`CriticalButton`,
   `VulkanViewport`, `Label`, `Clock`, `NumericDisplay`, `StatusIndicator`, `Panel`, `Image`),
   each added by an ADR for its immediate need. That worked while every widget was an output.
   Adding *interactive* widgets without organizing principles invites the classic failure modes:
   retained widget objects with framework-owned state, trait-object hierarchies that defeat
   static verification, or runtime view trees that violate the ADR-008 boundary.
3. **No editable text exists, and the text governance cannot simply be "opened up".** ADR-001
   through ADR-004 and ADR-010 deliberately exclude runtime shaping, fallback and free-form
   Unicode from the device path. A text-entry widget must therefore be designed *inside* that
   boundary — a bounded charset over baked glyphs with a compile-time width budget — or it does
   not exist at all.

### Survey: five widget architectures under IEC 62304 Class C constraints

Before selecting principles, the mainstream architectures were evaluated against the criteria a
Class C software item imposes: **determinism** (same inputs ⇒ same frame), **bounded memory**
(no runtime allocation, capacities fixed at startup), **auditability** (state and structure that
can be inspected, pinned and diffed as evidence), **static verifiability** (layout and content
checked at build time, per ADR-010/ADR-014), **state-transition testability** (widget state
machines exercisable without a GPU or an event loop), and **SOUP footprint** (ADR-005).

**SwiftUI** rebuilds value-type view trees whenever `@State`/`@Observable` data changes, then
diffs and re-solves layout at runtime. The attribute graph that stores view identity and state is
framework-owned and opaque: it cannot be inspected, sized in advance, or pinned as evidence.
Runtime layout solving is exactly what ADR-008 excludes, and hidden framework state fails
auditability outright. *Retained idea: the declarative description of a screen is separate from
render state — a principle this project already applies, one stage earlier, in the `.medui`
compiler.*

**Jetpack Compose** rewrites composable functions through a compiler plugin and re-executes them
on state change ("recomposition"), with `remember` values stored in a positional slot table. The
slot table is retained, positional, framework-owned memory; recomposition scheduling is skippable
and order-undefined by contract; the model allocates freely. All three properties are
disqualifying here. *Retained idea: **state hoisting** — a well-behaved composable owns no state;
its value comes from the caller and its events flow up to the caller. This is the exact
ownership discipline selected below.*

**React** re-renders component functions into a virtual DOM, diffs it against the previous tree,
and reconciles with heuristics; hooks attach state to components keyed by call order; memory is
garbage-collected. Per-frame tree construction plus GC fails bounded memory, and reconciliation
heuristics fail determinism. *Retained idea: **one-way data flow** and the **controlled
component** — the application owns an input field's value; the widget merely displays the value
it is given and emits change events. `TextInput` below is a controlled component in exactly this
sense.*

**Flutter** runs three trees: immutable widget descriptions, a retained mutable element tree
holding state, and render objects performing runtime layout. The widget tree's immutability is
attractive, but the element tree is precisely the retained, framework-owned, allocation-heavy
runtime structure this project must not have, and the render pipeline re-solves layout every
build. *Retained idea: the screen description should be cheap and immutable — this project
hoists that description all the way to compile time, where it becomes `const` data and golden
evidence.*

**Immediate-mode UIs** (Dear ImGui, egui) take the opposite stance: no retained widget objects
exist at all. The application re-declares the entire UI every frame from its own state; widget
functions return interactions directly (`if ui.button("Ack").clicked { … }`); the renderer is
stateless and consumes a flat draw list. The principles worth naming: **the application owns all
state**, **data flows through the frame, not through objects**, and **what is drawn is a pure
function of this frame's inputs** — trivially deterministic, trivially testable. The costs are
equally clear: layout is recomputed every frame, widget identity comes from hashing, real
implementations allocate per frame, and — decisively for a certified device — **there is no
static structure to pin as golden evidence**. A Class C screen cannot be "whatever the frame
closure emitted this time"; ADR-011/ADR-014 golden references demand a structure that exists
before the first frame runs. *Retained ideas: app-owned state and the per-frame data plane.
Rejected idea: per-frame structure.*

The conclusion of the survey is that this project's existing architecture is already the correct
hybrid, applied so far to output only: **structure retained at compile time** (the compiled
screen package — more static than any of the four frameworks' retained trees, and pinned as
evidence), combined with **an immediate-mode data plane** (the bounded `FrameInputs` mailbox
rewritten every frame from application state, per ADR-013). What is missing is the symmetric
half: an equally bounded event plane flowing the other way. This ADR formalizes the doctrine and
extends it to input.

## Decision

### 1. The organizing principles

These principles are normative for every current and future widget:

- **Structure is retained at compile time, and only there.** Widgets are variants of the closed
  `CompiledNodeKind` enum with `const`-constructible spec structs — never trait objects, never
  runtime-constructed trees. The widget set is closed; adding a widget is a change to the model,
  the compiler and the adapter, gated by an ADR. Composition happens in the DSL at authoring
  time; the `.medui` compiler is the only "recomposer" and it runs once, at build.
- **State lives in the application.** The framework retains no widget state. Application data
  flows one way **in** through `FrameInputs` (unchanged, ADR-013), and interaction events flow
  one way **out** through a new bounded `FrameEvents` queue. There is no callback registration,
  no observer graph, no framework-side dirty tracking.
- **The renderer stays stateless about widgets.** What a widget shows each frame is a pure
  function of the compiled package plus that frame's `FrameInputs`. A `TextInput`'s content is
  echoed into the frame by the application every frame, like a `NumericDisplay` value; the
  renderer never stores it. Transient *presentation* state (cursor position, pressed node, focus
  slot, caret) is a fixed-size structure owned by the adapter's event loop — platform-fed, like
  the `Clock`'s time source, and never application-semantic.
- **Everything is bounded.** Event queues have fixed capacities allocated at construction; text
  buffers are pre-reserved to a declared maximum; charsets are baked glyph sets; no per-frame
  heap allocation and no per-frame Vulkan object creation occur anywhere on the input path.

Nothing upstream is relaxed: the ADR-005 crate boundary (all winit handling stays in the
adapter), the ADR-008 DSL boundary, the ADR-013 `FrameInputs` contract and the ADR-014
verification rules all stay unchanged.

### 2. `FrameEvents` — the bounded outbound event plane

The `trustsc` facade gains `FrameEvents`, the mirror image of `FrameInputs`: a queue of
`WidgetEvent` values with capacity allocated once at construction and **no allocation
afterwards**. The event vocabulary:

```rust
pub enum WidgetEvent {
    ButtonPressed { source: &'static str },
    CriticalButtonPressed { node_id: &'static str, action: SystemEvent },
    CharTyped { source: &'static str, character: char },
    Backspace { source: &'static str },
    Delete { source: &'static str },
    CaretMoved { source: &'static str, position: u16 },
    TextCommitted { source: &'static str },
    FocusChanged { source: Option<&'static str> },
}
```

On overflow the newest event is dropped and a saturating `dropped_events` counter increments;
the counter is surfaced through the diagnostics so a dropped burst is a visible, auditable fact,
never a silent one. The adapter fills the queue from hit-tested winit events; the application
drains it exactly once per frame through a closure registered with
`App::with_input(|events, inputs| …)`, which runs before the existing `with_realtime` closure.
Applications that register no input closure compile and behave exactly as before.

### 3. `Button` — application-semantic interaction

`Button` is the general-purpose interactive widget. Its spec:

```rust
pub struct ButtonSpec {
    pub text_key: &'static str,
    pub color_token: &'static str,
    pub source: &'static str,
    pub requirement_id: Option<&'static str>,
}
```

The label is static approved text riding the existing `text_key()` path (budget-checked like a
`Label`); the face is a themed rectangle; the pressed tint is presentation state derived at
binding time. A press is delivered to the application as `ButtonPressed { source }` — by data,
not by callback. **`Button` carries no `SystemEvent`**: declaring `on_press` on a `Button` is a
compile error. What a press *means* belongs to the application; what the framework guarantees is
that the press was delivered through a bounded, inspectable channel.

### 4. `CriticalButton` gains real dispatch

The roles split: `Button` is application-semantic, `CriticalButton` is framework-governed. Its
spec, DSL grammar and golden-reference semantics stay unchanged, and its `on_press` finally
acquires runtime meaning:

- `SystemEvent.TriggerHalt` is dispatched by the framework itself: the adapter records a
  `Runtime`-category audit event naming the node and then performs an orderly event-loop exit —
  the halt is evidence, not just behavior. This requires the adapter to keep an audit handle
  alive inside the windowed loop (today the framework is dropped after startup diagnostics),
  reusing the `Framework::record_runtime_event` seam introduced by ADR-013.
- `SystemEvent.NoOp` is forwarded to the application as
  `CriticalButtonPressed { node_id, action }` and nothing else happens framework-side.

### 5. `TextInput` — a controlled component over a baked charset

`TextInput` is the first widget whose content is written by the operator, and it is designed as
a **controlled component** (the React sense) inside the ADR-001..004/ADR-010 text boundary:

```rust
pub struct TextInputSpec {
    pub source: &'static str,
    pub max_length: u16,
    pub glyph_set_id: &'static str,
    pub color_token: &'static str,
    pub requirement_id: Option<&'static str>,
}
```

- **The application owns the buffer.** The facade provides `TextInputModel`, a bounded editing
  helper with full caret navigation from the first version: insert-at-caret, `Backspace`,
  `Delete`, arrow-key caret movement, Home/End. Its buffer is reserved to `max_length` at
  construction and never reallocates. The adapter emits the editing events of §2; the
  application applies them to its model (or its own equivalent) and echoes the result every
  frame via `FrameInputs::set_text(source, &str)` — which rejects unknown sources, over-length
  content and characters outside the declared glyph set with typed errors. The renderer draws
  what it is handed and stores nothing.
- **The charset is a baked glyph set.** `SET-ASCII-TEXT` — the printable ASCII range — joins the
  standard text package through the same numeric-glyph-set mechanism that already carries the
  clock's digits, `:`, `-` and space. No shaping, no fallback, no IME, no Unicode composition:
  a character outside the set is rejected at the `set_text` boundary, deterministically.
- **The width budget is compile-time.** The DSL compiler enforces
  `max_length × widest-glyph-advance ≤ width` for the declared glyph set — the ADR-010/ADR-014
  budget doctrine applied to operator-typed content, exactly as the `NumericDisplay` fit check
  applies it to template-formatted numbers. An over-budget `max_length` is a compile error, not
  a clipped field on a bench.
- **Golden references pin the frame, not the content.** Like `NumericDisplay`, a `TextInput`'s
  golden reference carries bounds and color token with `text_key: None` — the varying content is
  governed by the bounded runtime path, the pinned box by the static evidence.

### 6. Focus and pressed state

Interaction bookkeeping lives in a fixed-size `InteractionState` inside the adapter's event
loop: the last cursor position, the currently armed button (pressed but not yet released), and a
**single focus slot** for text inputs. Click focuses the input under the cursor, `Tab` cycles
focus in document order, `Escape` clears it; focus changes are reported as
`FocusChanged { source }`. The caret is a solid rectangle drawn from the panel path — no
blinking, because a frame's content must be a pure function of that frame's inputs, not of a
wall-clock phase. Pressed and focused tints are derived once at binding time from the theme
table (ADR-014); no color is computed per frame.

## Consequences

- The widget set stays closed and every future widget is a compiler-and-ADR change. That is the
  intended cost structure for a Class C UI: widget growth is deliberate, reviewed and evidenced,
  never incidental.
- Text entry is deliberately modest: printable ASCII, fixed maximum length, no IME, no Unicode
  composition — far below any consumer framework, and appropriate for device UIs whose free-form
  entry is identifiers, not prose. A smaller uppercase-plus-digits charset was considered and
  rejected in favor of one reusable printable-ASCII bake.
- Full caret editing in the first version costs more adapter and model surface than an
  append-only field would have, but avoids a breaking change to the editing model and its event
  vocabulary later.
- The event queue can drop events under a burst; the drop is counted and surfaced rather than
  prevented. Boundedness is chosen over losslessness, consistently with ADR-013.
- The adapter grows pointer, keyboard, hit-testing and focus logic, but remains the only crate
  touching winit (ADR-005 intact), and the input path adds nothing to the headless-smoke path.
- Existing applications compile unchanged: `with_realtime` keeps its signature, `with_input` is
  additive, and screens without interactive widgets emit no events.

## References

- ADR-005 (dependency boundary) — all winit input handling stays in the adapter
- ADR-008 (deterministic MedUI DSL boundary) — structure stays compile-time; the DSL gains
  widgets, not control flow
- ADR-010 (i18n and text budget policy) — the `TextInput` width budget extends it to typed
  content
- ADR-011 (safety monitor contract) — golden-reference semantics for the new kinds
- ADR-013 (bounded realtime contract) — `FrameEvents` mirrors `FrameInputs`' bounded discipline
- ADR-014 (precise positioning and theme colors) — placement verification and tint derivation
  for the new widgets
- Epic #72
