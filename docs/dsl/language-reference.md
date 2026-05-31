# MedUI DSL language reference

## Supported constructs in the first slice

```text
Screen <Name> {
    layout: Vertical { spacing: 16px; padding: 24px; }

    @safety_critical(cv_check: [Bounds, ColorHash])
    CriticalButton {
        id: hello-world-label;
        requirement: "REQ-HELLO-001";
        width: Fill;
        height: 80px;
        label: t("STR-HELLO-WORLD");
        color: Theme.Colors.PrimaryAction;
        on_press: SystemEvent.NoOp;
    }

    VulkanViewport {
        id: hello-world-viewport;
        width: Fill;
        height: 280px;
        stream_source: "HELLO_WORLD_SIM";
    }
}
```

## Rules

- ids use ASCII alphanumeric characters, `_`, or `-`
- sizes use `Npx` or `Fill`
- text uses `t("key")`
- safety annotations apply to the next component block only
- `CriticalButton` requires `requirement`, `label`, `color`, and `on_press`
- `VulkanViewport` requires `stream_source`

## Forbidden in the first slice

- loops
- conditionals
- recursion
- runtime scripts
- nested layout containers
- hardcoded product strings
