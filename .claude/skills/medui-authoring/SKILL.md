---
name: medui-authoring
description: Author or modify .medui UI screens — the build-time-only MedUI DSL (Screen, Vertical/Horizontal, CriticalButton, VulkanViewport, SignalTrace, @safety_critical, t("key")). Use when creating a screen, changing layout/widgets, adding localized text, or wiring a screen into an application's build.rs.
---

# MedUI authoring

`.medui` files are compiled at build time into a static `CompiledScreenPackage` — nothing is
parsed, laid out, or shaped on-device (ADR-008/009). The full reference lives in `docs/dsl/`:
`overview.md`, `language-reference.md`, `component-dictionary.md`, `safety-monitor-contract.md`,
`build-integration.md`. Worked examples: `examples/hello_world/hello_world.medui` (minimal) and
`examples/class_c_monitor/` (full NeuroSense 500 screen).

## Build wiring

1. Author `<app>/<name>.medui`.
2. In the app's `build.rs`:
   ```rust
   mdux_build::MeduiScreen::new("path/to/name.medui")
       .surface(width, height)
       .compile();
   ```
   (`mdux-build` handles `OUT_DIR` and `rerun-if-changed`.)
3. In the app: `mdux::include_medui_screen!();` — exposes the generated module as `medui_screen`;
   `medui_screen::screen()` returns the `&'static CompiledScreenPackage`.
4. `FrameworkBuilder::with_screen(screen)` auto-derives a `UiComponent` per requirement-bearing
   node — don't hand-write `UiComponent`s for screen nodes.

## Rules the compiler enforces (design for them up front)

- **Text**: all user-visible strings go through `t("key")` against the approved text package.
  A node is rejected if its bounds are too small for the **widest approved translation across all
  locales** (ADR-010) — size components for the worst-case locale, not English.
- **`@safety_critical`** nodes get golden-reference entries baked into the package and are
  checked by `--verify-ui` (ADR-011/016). Use it for any node whose misrendering is a hazard.
- **Positioning**: `position:` is pixel-exact within the declared surface (ADR-014); container
  nodes (`Vertical`/`Horizontal`) handle flow layout. Theme colors come from the token set in
  `docs/dsl/component-dictionary.md` — no ad-hoc RGB in screens.
- **Realtime data** (waveforms, numeric readouts) enters only through the bounded `FrameInputs`
  plane at frame time (`VulkanViewport`, `SignalTrace`, `NumericDisplay`, `StatusIndicator`) —
  the compiled screen itself is immutable.

## After editing a screen

```bash
cargo build --locked -p <app>          # recompiles the .medui; DSL errors surface here
cargo run --locked -q -p <app> -- --headless-smoke
cargo run --locked -q -p <app> -- --verify-ui=generated/verification --locales=all
```

If a new widget kind or DSL construct is needed, that's an ADR-level change (see ADR-015 widget
organization principles and ADR-018 for the precedent of adding `SignalTrace`), not a quick edit.
