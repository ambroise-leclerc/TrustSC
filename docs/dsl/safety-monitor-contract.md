# MedUI DSL safety-monitor contract

## `@safety_critical`

The first slice supports:

```text
@safety_critical(cv_check: [Bounds, ColorHash])
```

This emits a golden-reference entry with:

- `node_id`
- `bounds`
- `text_key` when applicable
- `color_token` when applicable
- `cv_checks`

## Current runtime/governance expectation

- a safety-critical component still requires an explicit `requirement` property
- the generated golden reference is static build output
- the current demo exposes the generated entries for inspection and automated tests

Future work may add generated C-compatible tables for external safety monitors.
