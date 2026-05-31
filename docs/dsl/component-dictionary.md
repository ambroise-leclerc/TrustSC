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
