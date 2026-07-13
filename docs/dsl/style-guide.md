# MedUI DSL Style Guide

## Indentation

- Use 4-space indentation.

## Property Lines

- Each property line ends with a semicolon (`;`).

## Layout Declaration

- The `layout:` header is on one line.
- Braces are on their own lines.
- Example:
  ```medui
  layout: Vertical {
      spacing: 8px;
      padding: 0px;
  }
  ```

## Surface Declaration

- The `surface:` declaration is immediately after the `layout:` header.
- Example:
  ```medui
  surface: 1280px, 720px;
  ```

## Safety Critical Annotation

- The `@safety_critical(...)` annotation is on its own line directly above its component.
- Example:
  ```medui
  @safety_critical("requirement_id")
  CriticalButton {
      id: "button1";
      requirement: "req123";
      width: 100px;
      height: 50px;
      label: t("button_label");
      color: "primary";
      on_press: SystemEvent::Acknowledge;
  }
  ```

## Blank Lines

- Use a blank line between components.

## Comments

- Use `//` for single-line comments.
- Example:
  ```medui
  // This is a comment
  CriticalButton {
      id: "button1";
      requirement: "req123";
      width: 100px;
      height: 50px;
      label: t("button_label");
      color: "primary";
      on_press: SystemEvent::Acknowledge;
  }
  ```

## Canonical Property Order

- The canonical property order is as follows:
  - `id`
  - `width`
  - `height`
  - `position`
  - Kind-specific properties (e.g., `requirement`, `label/text/template`, `source`, `states`, `colors`, `stream_source`, `format`, `max_length`, `charset`, `color`, `on_press`)

- For `Row` components:
  - `id`
  - `height`
  - `spacing`
  - `background`
  - Children

## Why This Matters

- Line-oriented canonical formatting gives single-line PR diffs (moving a node changes exactly one `position:` line), which is what makes `.medui` changes cheap to review in a regulated workflow.
