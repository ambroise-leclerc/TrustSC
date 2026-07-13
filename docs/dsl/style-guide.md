# MedUI DSL Style Guide

## Indentation

- Use 4-space indentation.

## Block/Bracing Constraints

- Each property must be on its own line.
- Braces `{ ... }` should be on their own lines.

## Property Lines

- Each property line ends with a semicolon (`;`).

## Layout Declaration

- The `layout:` header is on one line, followed by the entire `{ ... }` block on the same line as `layout:`.
- Example:
  ```medui
  layout: Vertical { spacing: 8px; padding: 0px; }
  ```

## Surface Declaration

- The `surface:` declaration is optional and should appear right after the `layout:` line (before the first component).
- Example:
  ```medui
  surface: 1280px, 720px;
  ```

## Safety Critical Annotation

- The `@safety_critical(...)` annotation is on its own line directly above its component.
- Example:
  ```medui
  @safety_critical(cv_check: [Bounds, ColorHash])
  CriticalButton {
      id: button1;
      requirement: "REQ-123";
      width: 100px;
      height: 50px;
      label: t("button_label");
      color: Theme.Colors.Primary;
      on_press: SystemEvent.Acknowledge;
  }
  ```

## Blank Lines

Blank lines between top-level components are recommended for readability. The committed example screens vary in their use of blank lines; however, consistency is encouraged.

## Comments

- Use `//` for single-line comments.
- Example:
  ```medui
  // This is a comment
  CriticalButton {
      id: button1;
      requirement: "REQ-123";
      width: 100px;
      height: 50px;
      label: t("button_label");
      color: Theme.Colors.Primary;
      on_press: SystemEvent.Acknowledge;
  }
  ```

## Canonical Property Order

The canonical property order is per component kind, cross-checked against `docs/dsl/component-dictionary.md` and the committed examples. For components where `requirement` is required (e.g., `CriticalButton`, `NumericDisplay`, `StatusIndicator`), it should be listed before other properties.

- For `CriticalButton` components:
  - `requirement`
  - `id`
  - `width`
  - `height`
  - `label`
  - `color`
  - `on_press`

- For `VulkanViewport` components:
  - `id`
  - `width`
  - `height`
  - `stream_source`

- For `SignalTrace` components:
  - `id`
  - `width`
  - `height`
  - `stream_source`
  - `color`

- For `Row` components:
  - `id`
  - `height`
  - `spacing`
  - `background`
  - Children

- For `Label` components:
  - `id`
  - `width`
  - `height`
  - `text`
  - `color`

- For `Clock` components:
  - `id`
  - `width`
  - `height`
  - `format`

- For `NumericDisplay` components:
  - `requirement`
  - `id`
  - `width`
  - `height`
  - `template`
  - `source`
  - `color`

- For `StatusIndicator` components:
  - `requirement`
  - `id`
  - `width`
  - `height`
  - `source`
  - `states`
  - `colors`

- For `Image` components:
  - `id`
  - `width`
  - `height`
  - `source`

- For `Button` components:
  - `id`
  - `width`
  - `height`
  - `label`
  - `color`
  - `source`
  - `requirement`

- For `TextInput` components:
  - `id`
  - `width`
  - `height`
  - `source`
  - `max_length`
  - `color`
  - `charset`
  - `requirement`

## Why This Matters

- Line-oriented canonical formatting gives single-line PR diffs (moving a node changes exactly one `position:` line), which is what makes `.medui` changes cheap to review in a regulated workflow.
