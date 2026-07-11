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

## `SignalTrace` (ADR-018)

```text
    SignalTrace {
        id: eeg-trace;
        width: Fill;
        height: 240px;
        stream_source: "EEG_TRACE";
        color: Theme.Colors.Nominal;
    }
```

## Interactive widgets (ADR-015)

```text
    Button {
        id: ack-button;
        width: 240px;
        height: 64px;
        position: 1392px, 720px;
        label: t("STR-NS-ACK");
        color: Theme.Colors.PrimaryAction;
        source: "ACK_BUTTON";
        requirement: "REQ-NS-004";
    }

    TextInput {
        id: patient-id-input;
        width: 512px;
        height: 48px;
        position: 1392px, 640px;
        source: "PATIENT_ID";
        max_length: 16;
        charset: AsciiText;
        color: Theme.Colors.Title;
        requirement: "REQ-NS-005";
    }
```

## Rules

- ids use ASCII alphanumeric characters, `_`, or `-`
- sizes use `Npx` or `Fill`
- text uses `t("key")`
- safety annotations apply to the next component block only
- `CriticalButton` requires `requirement`, `label`, `color`, and `on_press`
- `VulkanViewport` requires `stream_source`
- `SignalTrace` requires `stream_source` and `color` (ADR-018)
- `Row` requires `id` and `height`, contains leaf components only (one nesting level, `Vertical`
  screens only), and is resolved away at compile time — the emitted package stays flat
- `Label` requires `text` and `color`; `Clock` requires `format`
- `NumericDisplay` requires `requirement`, `template`, `source`, and `color`
- `StatusIndicator` requires `requirement`, `source`, and `states`; `colors` is optional
- `Image` requires `source: img("IMAGE-ID")` and dimensions equal to the baked image's
  intrinsic size (no scaling)
- `Button` requires `label`, `color`, and `source` (a quoted event key); `requirement` is
  optional; declaring `on_press` is a compile error — framework-governed system events belong
  to `CriticalButton` (ADR-015)
- `TextInput` requires `source`, `max_length` (a positive character count), and `color`;
  `charset` is optional (defaults to `AsciiText`, the printable-ASCII baked glyph set) and
  `requirement` is optional; the compiler enforces
  `max_length × widest-glyph-advance ≤ width` against the declared charset
- any leaf component may declare `position: <X>px, <Y>px` — absolute screen coordinates, out of
  flow, fixed sizes only; the compiler enforces containment, no-overlap, text budgets in the
  pinned box, and emits an automatic `Bounds` golden reference (ADR-014)
- a screen may pin its surface with `surface: <W>px, <H>px;` after the `layout:` line — a
  build configured for another surface fails the compile
- `Row` accepts `background: <token>;`, emitting a synthetic full-width `Panel` underlay
- every color token (`color`, `colors`, `background`) must exist in the governed
  `THEME_COLORS` table — unknown tokens fail the compile
- `0px` is legal for layout `spacing`/`padding` and `position` coordinates, never for component
  sizes
- lists use `[a, b, c]`; one property per line

## Forbidden in the first slice

- loops
- conditionals
- recursion
- runtime scripts
- layout containers nested deeper than one `Row` level
- hardcoded product strings
