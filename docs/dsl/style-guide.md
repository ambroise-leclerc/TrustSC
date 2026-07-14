# MedUI DSL Style Guide

This guide documents the canonical formatting of `.medui` files, derived from the committed
examples (`examples/hello_world/hello_world.medui`, `examples/class_c_monitor/neurosense.medui`)
and the parser's constraints (`docs/dsl/language-reference.md`).

## Indentation

- Use 4-space indentation.

## Line-Oriented Block Constraints

The parser is strictly line-oriented:

- Each property is on its own line and ends with a semicolon (`;`).
- The opening brace stays on the component line (`CriticalButton {`); the closing brace `}`
  is alone on its own line.
- The only inline-block exception is the one-line `layout:` header (see below).

## Layout Declaration

- The `layout:` header is a single line: the entire `{ ... }` block stays on the same line
  as `layout:`.
- Example:
  ```medui
  layout: Vertical { spacing: 8px; padding: 0px; }
  ```

## Surface Declaration

- The `surface:` declaration is optional and appears immediately after the `layout:` line
  (before the first component).
- Example (from `neurosense.medui`):
  ```medui
  surface: 1920px, 1080px;
  ```

## Safety Critical Annotation

- The `@safety_critical(...)` annotation is on its own line, directly above its component.
- Example (from `hello_world.medui`):
  ```medui
  @safety_critical(cv_check: [Bounds, ColorHash])
  CriticalButton {
      id: hello-world-label;
      requirement: "REQ-HELLO-001";
      width: Fill;
      height: 48px;
      label: t("STR-HELLO-WORLD");
      color: Theme.Colors.PrimaryAction;
      on_press: SystemEvent.NoOp;
  }
  ```

## Comments

- Use `//` for single-line comments, on their own line above the code they describe.
- Example:
  ```medui
  // Top bar: device title, wall clock, system status.
  Row {
  ```

## Blank Lines

Blank lines between top-level components are recommended for readability. The committed
example screens vary in their use of blank lines (`hello_world.medui` separates components
with blank lines, `neurosense.medui` does not); consistency within a file is what matters.

## Canonical Property Order

The general order is: `id`, `width`, `height`, `position`, then `requirement` (for
components that declare one), then kind-specific properties, ending with event handlers
(`on_press`). `position` appears in absolutely-positioned screens (e.g. `neurosense.medui`);
flow-layout screens (`hello_world.medui`) omit it. One committed exception: `CriticalButton`
places `requirement` right after `id`.

Per component kind, as used in the committed examples:

- `CriticalButton`: `id`, `requirement`, `width`, `height`, `label`, `color`, `on_press`
- `VulkanViewport`: `id`, `width`, `height`, `position` (when present), `stream_source`
- `SignalTrace`: `id`, `width`, `height`, `position`, `stream_source`, `color`
- `Row`: `id`, `height`, `spacing` (when used), `background`, then indented children
- `Label`: `id`, `width`, `height`, `position`, `text`, `color`
- `Clock`: `id`, `width`, `height`, `position`, `format`
- `NumericDisplay`: `id`, `width`, `height`, `position`, `requirement`, `template`, `source`, `color`
- `StatusIndicator`: `id`, `width`, `height`, `position`, `requirement`, `source`, `states`, `colors` (when used)
- `Image`: `id`, `width`, `height`, `position`, `source`
- `Button`: `id`, `width`, `height`, `position`, `requirement`, `label`, `color`, `source`
- `TextInput`: `id`, `width`, `height`, `position`, `requirement`, `source`, `max_length`, `charset`, `color`

## Why This Matters

Line-oriented canonical formatting gives single-line PR diffs (moving a node changes exactly
one `position:` line), which is what makes `.medui` changes cheap to review in a regulated
workflow.
