# Automated UI verification (ADR-016)

TrustSC validates layout at compile time (containment, overlap, per-locale text budgets —
[MedUI DSL reference](../dsl/overview.md)), but a compiled layout is a claim about what *should*
render. `--verify-ui` is the run mode that checks what actually did: it renders a screen offscreen
through the exact same command-recording path a real window uses, runs a pure pixel-level check
engine against the captured frame, and writes a deterministic JSON report plus PPM screenshots as
evidence. This page is the operator guide: how to run it, what each check means and how strict it
is, the report schema, and how to bootstrap and refresh the one check that needs a committed
baseline.

Golden references (`@safety_critical(cv_check: [...])` and every positioned node's automatic
`Bounds` reference — see [the safety-monitor contract](../dsl/safety-monitor-contract.md)) are
compiled build output before this page's machinery ever runs. `--verify-ui` is their first actual
*consumer*: the piece that turns a static reference sitting in a compiled package into a pass/fail
result against real rendered pixels.

## Running it

```sh
# a single locale (defaults to the app's configured locale if --locales is omitted)
cargo run -p hello_world -- --verify-ui=generated/verification --locales=en-US

# every locale the standard text package declares
cargo run -p class_c_monitor -- --verify-ui=generated/verification --locales=all

# an explicit list
cargo run -p class_c_monitor -- --verify-ui=generated/verification --locales=en-US,fr-FR

# only one registered scenario (App::with_scenarios), instead of every one
cargo run -p class_c_monitor -- --verify-ui=generated/verification --scenario=ack-flow
```

`--verify-ui` and `--headless-smoke` are mutually exclusive run modes on the same `App`.
`--verify-ui` exits non-zero if any check or scenario step failed, but only *after* writing every
report — a CI failure still leaves the full evidence set to download and inspect.

Requires a Vulkan ICD (CI uses lavapipe, Mesa's software rasterizer — `sudo apt install
mesa-vulkan-drivers`; a real GPU driver also works locally, see below).

### Output layout

```text
generated/verification/<software-item>/<locale>/
    report.json          # schema v1, see below
    screenshot.ppm        # the base (non-scenario) capture, binary PPM (P6) — RGB only, no alpha
    step-<scenario-id>-<label>.ppm   # one per scenario capture step, when scenarios ran
generated/verification/<software-item>/baselines/lavapipe/<locale>.txt
    # committed lavapipe ColorHash baselines — see "Tier 2" below; empty/absent until bootstrapped
```

`<software-item>` is the device's configured software item (`DeviceContext`) — not the screen id
(two applications can compile the same screen id) and not the crate/version-derived framework
identity, which is shared by every application. `generated/verification/` is **not** committed
evidence like the baked font/image/model packages elsewhere under `generated/`: it is
`.gitignore`d, regenerated fresh on every run, and uploaded as a CI artifact
(`ui-verification-evidence`) rather than checked in. The one thing deliberately promoted out of
that tree into a real committed gate is the lavapipe `ColorHash` baseline file, and that promotion
is a manual `git add -f`, never automatic — see [Tier 2](#tier-2-color_hash-exact-lavapipe-only).

## Check vocabulary

Every check in `crates/trustsc-ui-verify` is pure (no GPU, no window, unit-testable on synthetic
pixel buffers) and falls into one of two tiers.

### Tier 1: property checks (every backend, every run)

These are the certified gate — they run everywhere, on any Vulkan implementation, and encode a
*property* of correct rendering rather than an exact pixel match, so they aren't sensitive to
driver-level antialiasing/rounding differences between backends.

| Check | What it measures | Applies to | Tolerance |
|---|---|---|---|
| `golden_bounds` | Every ink pixel found in a golden-reference node's search region (declared bounds + 8px margin) stays inside the declared bounds. Background is resolved per-pixel against the node's own chrome or an enclosing Panel's fill, so a label sitting on a themed panel isn't misjudged. No ink found at all is vacuously contained (a node can legitimately be full-bleed its own background color). | Every golden-reference entry (`@safety_critical` and every positioned node — ADR-014) | "ink" = any RGBA channel differing from the resolved background by more than 8 UNORM steps |
| `chrome_color` | A node's own solid, glyph-free fill matches its resolved theme color. Sampled from edge-inset bands that stay clear of any centered label/caret (`Panel`, `Button`/`CriticalButton` face, `TextInput` field — the latter checked against `Theme.Colors.Neutral` scaled by 0.35, the adapter's actual unfocused-field tint, not the node's own `color_token`, which tints the caret). Not run for kinds with no solid glyph-free region to sample (`Label`, `Clock`, `NumericDisplay`, `Image`, `VulkanViewport`, `SignalTrace`) or whose active color depends on runtime state this pure engine doesn't carry (`StatusIndicator`). | `Panel`, `Button`, `CriticalButton`, `TextInput` | ≤1 UNORM step per channel; zero samples is always a fail, never a vacuous pass |
| `text_presence` | Ink coverage (parts-per-million of the node's own bounds area) falls inside a generous heuristic band derived from the compiled glyph count — wide enough to span condensed/bold fonts and dense punctuation without false-failing on any real glyph set, while still catching a blank region or garbled/overflowing text. Only checked for statically-known text (`Label`/`Button`/`CriticalButton`) — dynamic content (`Clock`, `NumericDisplay`, `StatusIndicator`, `TextInput`) has no compile-time glyph count to derive a band from. | Static text-bearing nodes | see `coverage_band_for_glyphs` in `crates/trustsc-ui-verify/src/lib.rs` |
| `ink_containment` | Zero ink pixels found outside a node's own bounds within an 8px margin, excluding pixels that fall inside an adjacent node's own (ADR-014-disjoint) bounds. | Every compiled node | 8px search margin |

### Tier 2: `color_hash` (exact, lavapipe-only)

`ColorHash` is a SHA-256 of the golden rect's raw RGBA bytes at the authored extent — an *exact*
byte comparison, not a tolerance-based property check, so it can only ever be meaningful against a
baseline captured on the identical rendering backend. Baselines are **lavapipe-only**: on any other
backend (a real GPU driver, a different lavapipe version) the check reports `no_baseline`, which is
informational, never a pass — `CheckOutcome::is_pass()` is the only predicate a gate should use,
and it returns `false` for `no_baseline` exactly like it does for `fail`. This is the honesty
guarantee ADR-016 §3 is about: a `ColorHash` check never silently passes just because there was
nothing to compare against.

**Baseline bootstrap.** The first lavapipe run for a given `(software item, locale)` finds no
baseline file and every `ColorHash` check on it reports `no_baseline`; `--verify-ui` then writes
`baselines/lavapipe/<locale>.txt` (tab-separated `node_id\thex` lines, sorted, one row per golden
reference on the screen) from what it just measured — self-bootstrapping, but a human still has
to review and commit the result:

```sh
cargo run -p class_c_monitor -- --verify-ui=generated/verification --locales=all
git add -f generated/verification/neurosense-ui/baselines/lavapipe/
git commit -m "Bootstrap lavapipe ColorHash baselines for NeuroSense500"
```

**Refresh** (after an intentional visual change): delete the stale baseline file and rerun the
same command — a fresh bootstrap replaces it. Once a baseline file exists and is committed, every
future lavapipe run on that `(item, locale)` compares against it: an unintended color
regression turns into a real `fail`, an intentional one needs the same delete-and-rebootstrap step
with the diff reviewed like any other change to committed evidence.

**As of this writing, no baseline files are committed in this repository** — `--verify-ui` in CI
therefore bootstraps fresh on every run and `color_hash` never actually gates a regression there
yet. Enabling real Tier-2 enforcement for an application is exactly the two-command bootstrap above,
done once and reviewed like any other evidence commit.

## Report schema (v1)

`crates/trustsc-ui-verify::emit_report_json` renders `report.json` by hand — no serde in governed
or adapter code, an explicit decision in
[ADR-016](../adr/ADR-016-automated-ui-verification-and-manual-generation.md) §5: fixed key order,
integers only (coverage as parts-per-million, never floats — determinism across platforms), LF
line endings, no trailing whitespace, a trailing newline. Byte-reproducible given identical
inputs, so a report is diffable and can itself be byte-compared like any other evidence artifact.

Top-level fields:

| Field | Meaning |
|---|---|
| `schema_version` | `1` |
| `report_kind` | always `"trustsc-ui-verification"` |
| `tool_name`, `tool_version` | `trustsc-ui-verify`'s own crate identity |
| `software_item`, `safety_class` | from the device's `DeviceContext` |
| `screen_id`, `locale` | which compiled screen and which locale this report captures |
| `surface_width`, `surface_height` | the captured frame's pixel extent (the screen's authored surface) |
| `backend_id`, `device_name` | the rendering backend (`"lavapipe"` when it is; a normalized GPU name otherwise) and the raw Vulkan device name |
| `pixel_format` | always `R8G8B8A8_UNORM` (the offscreen path's fixed capture format) |
| `clock` | the pinned `WallClock` this capture used, ISO 8601 — determinism for `Clock` nodes |
| `screenshot_file`, `screenshot_sha256` | the base capture's PPM filename and its digest |
| `checks[]` | every `CheckResult` — see below |
| `scenario_traces[]` | one row per scripted-scenario step: `scenario_id`, `step_index`, `description`, `expected`, `observed`, `passed` |
| `trace_rows[]` | the REQ → VER → check join — see below |

Each entry in `checks[]`:

| Field | Meaning |
|---|---|
| `check_id` | `"<node-id>::<check-kind>"`, or `"<scenario-id>::<step-label>::<node-id>::<check-kind>"` for a scenario-step check |
| `node_id` | the compiled node this check ran against |
| `kind` | one of `golden_bounds`, `chrome_color`, `text_presence`, `ink_containment`, `color_hash` |
| `requirement_id` | the node's `requirement:` property, or `null` for kinds that don't declare one (e.g. `VulkanViewport`) |
| `outcome` | `pass`, `fail`, or `no_baseline` (`color_hash` only) |
| `payload` | outcome-specific measured/expected values — shape varies by `kind`, see the worked example below |

`trace_rows[]` joins governed requirements to their verifications and the concrete checks that
exercised them: `requirement_id`, `verification_ids[]` (from
[`trustsc-governance`](../architecture.md)'s `ComplianceProgram::trace_rows()`), `check_ids[]`
(every `check_id` above whose `requirement_id` matches). This is the row to attach to a VER-xxx
verification case as automated evidence: it names exactly which checks, against exactly which
pixels, exercised exactly which requirement.

### Worked example

Produced by `cargo run -p hello_world -- --verify-ui=generated/verification --locales=en-US` on a
real GPU backend (trimmed to two checks; a real report has one entry per applicable check per
node):

```json
{
  "schema_version": 1,
  "report_kind": "trustsc-ui-verification",
  "tool_name": "trustsc-ui-verify",
  "tool_version": "0.1.0",
  "software_item": "hello-world-ui",
  "safety_class": "Class B",
  "screen_id": "HelloWorld",
  "locale": "en-US",
  "surface_width": 800,
  "surface_height": 480,
  "backend_id": "nvidia-geforce-rtx-4060-ti",
  "device_name": "NVIDIA GeForce RTX 4060 Ti",
  "pixel_format": "R8G8B8A8_UNORM",
  "clock": "2026-01-01T12:00:00Z",
  "screenshot_file": "screenshot.ppm",
  "screenshot_sha256": "099963ed7405d2effffb1e479496da8fffb8cdcb192bbe67e7fbd5c59c6939b1",
  "checks": [
    {
      "check_id": "hello-world-label::chrome_color",
      "node_id": "hello-world-label",
      "kind": "chrome_color",
      "requirement_id": "REQ-HELLO-001",
      "outcome": "pass",
      "payload": {
        "expected_token": "Theme.Colors.PrimaryAction",
        "expected_rgba": [41, 112, 219, 255],
        "measured_rgba": [41, 112, 219, 255],
        "max_channel_delta": 0,
        "sample_count": 8928
      }
    },
    {
      "check_id": "hello-world-label::color_hash",
      "node_id": "hello-world-label",
      "kind": "color_hash",
      "requirement_id": "REQ-HELLO-001",
      "outcome": "no_baseline",
      "payload": {
        "expected_hex": null,
        "measured_hex": "f022130cea4a377c7b76fb9c251c78860b4600da4824ff5934eee53acd275b9d"
      }
    }
  ],
  "scenario_traces": [],
  "trace_rows": [
    {
      "requirement_id": "REQ-HELLO-001",
      "verification_ids": ["VER-HELLO-001"],
      "check_ids": [
        "hello-world-label::chrome_color",
        "hello-world-label::ink_containment",
        "hello-world-label::text_presence",
        "hello-world-label::golden_bounds",
        "hello-world-label::color_hash"
      ]
    }
  ]
}
```

`color_hash` reports `no_baseline` here because this capture ran on a real GPU backend, not
lavapipe — exactly the Tier-2 honesty behavior described above, not a bug.

## Scenarios (ADR-016 §4)

A scenario is a scripted sequence of input events plus state assertions, registered via
`App::with_scenarios` and compiled from TOML at build time
([build integration](../dsl/build-integration.md)'s codegen doctrine — no runtime TOML parsing).
Replay is GPU-free event injection; only `capture` steps render and run the full pixel check suite
against that step's frame, writing `step-<scenario-id>-<label>.ppm` alongside the base screenshot
and prefixing every check's `check_id` with `<scenario-id>::<label>::`. A scenario step's own
expected/observed state assertion (independent of the pixel checks) lands in `scenario_traces[]`.

## CI

`.github/workflows/ci.yml` installs lavapipe, then runs `hello_world` and `class_c_monitor`
through `--verify-ui` (`class_c_monitor` covers `--locales=all`; `hello_world`'s greeting is only
authored in `en-US`, so it only needs to prove the single-locale path) and uploads
`generated/verification/` as the `ui-verification-evidence` artifact — always, even on failure, so
a red build still has the full report/screenshot set to inspect. Replay the same checks locally
with the commands in [AGENTS.md](../../AGENTS.md#commands)'s CI-replay block.
