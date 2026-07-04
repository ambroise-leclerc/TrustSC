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

Per-kind semantics of the optional fields:

| Kind | `text_key` | `color_token` |
|---|---|---|
| `CriticalButton` | its label key | its color token |
| `Label` | its text key | its color token |
| `NumericDisplay` | `None` (digits vary at runtime by design) | its color token |
| `StatusIndicator`, `Clock`, `VulkanViewport` | `None` | `None` |
| `Image` | `None` (pixel content is baked evidence, not text) | `None` |

Dynamic kinds pin their *bounds* (and color where meaningful): the reference tells the safety
monitor **where** critical content must appear and in what tint — the varying content itself is
governed by the bounded realtime path (ADR-013), not by a static reference.

## Positioned nodes are golden evidence (ADR-014)

Every node with an explicit `position:` receives an **automatic** golden reference carrying
`Bounds`, even without `@safety_critical` — a declared position is a safety-relevant claim and
becomes reproducible, machine-checkable evidence. When a positioned node also carries
`@safety_critical`, the compiler emits **one merged entry** (deduplicated union of cv_checks),
never two entries per node id. Synthetic `Panel` background nodes never receive golden
references (underlays by definition).

## Current runtime/governance expectation

- a safety-critical component still requires an explicit `requirement` property
- the generated golden reference is static build output
- the current demo exposes the generated entries for inspection and automated tests

Future work may add generated C-compatible tables for external safety monitors.
