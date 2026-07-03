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

## Monitor widgets and the `Row` container

```text
Screen NeuroSense500 {
    layout: Vertical { spacing: 8px; padding: 16px; }

    Row {
        id: topbar;
        height: 48px;
        spacing: 16px;
        Label {
            id: device-title;
            width: 340px;
            height: 48px;
            text: t("STR-NS-TITLE");
            color: Theme.Colors.Title;
        }
        Clock {
            id: wall-clock;
            width: Fill;
            height: 48px;
            format: DateTimeSeconds;
        }
        StatusIndicator {
            id: system-status;
            width: 200px;
            height: 48px;
            requirement: "REQ-NS-003";
            source: "MONITOR_STATUS";
            states: [t("STR-NS-NOMINAL"), t("STR-NS-ALERT"), t("STR-NS-FAULT")];
        }
    }

    @safety_critical(cv_check: [Bounds, ColorHash])
    NumericDisplay {
        id: sedation-index;
        width: Fill;
        height: 120px;
        requirement: "REQ-NS-001";
        template: "TPL-SEDATION-INDEX";
        source: "SEDATION_INDEX";
        color: Theme.Colors.ScoreDigits;
    }

    VulkanViewport {
        id: eeg-dsa;
        width: Fill;
        height: Fill;
        stream_source: "EEG_DSA";
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
- `Row` requires `id` and `height`, contains leaf components only (one nesting level, `Vertical`
  screens only), and is resolved away at compile time — the emitted package stays flat
- `Label` requires `text` and `color`; `Clock` requires `format`
- `NumericDisplay` requires `requirement`, `template`, `source`, and `color`
- `StatusIndicator` requires `requirement`, `source`, and `states`; `colors` is optional
- lists use `[a, b, c]`; one property per line

## Forbidden in the first slice

- loops
- conditionals
- recursion
- runtime scripts
- layout containers nested deeper than one `Row` level
- hardcoded product strings
