# ADR-022: MedUI Studio — a hosted web service for interactive `.medui` design

## Status

Accepted

## Context

No `.medui` design tooling exists today: no previewer, no LSP, no formatter, no editor. The only
way to see or change a screen is to hand-edit text and rebuild an example application. That is
fine for a developer comfortable with the DSL, but it blocks the actual goal: a manager or
product owner should be able to design or tweak a screen **without a developer**, with the result
still flowing through the normal regulatory gate — a `.medui` file proposed as a pull request,
verified by `--verify-ui` in CI (ADR-016) exactly like a hand-written one.

The governed closed sets make this unusually tractable compared to general-purpose UI design
tooling: there are 10 widget kinds (`NodeKind`), a fixed `THEME_COLORS` token table, approved
`t("key")` text with per-locale width budgets (ADR-010), and baked images with intrinsic sizes
(ADR-014). Every design choice a user can make is a pick from an approved, enumerable list, and
the existing compiler is a ready-made validator — a designer surface only has to *offer* those
lists and *call* the compiler, not reimplement its rules.

Four form factors were considered:

- **Native desktop app** (egui/eframe, single binary). Smallest SOUP delta and no server to run,
  but distribution to a non-developer manager (build, sign, update a binary per platform) is
  exactly the friction this project wants to remove, and it still requires local Vulkan/lavapipe
  setup to get pixel-exact preview.
- **Local web app** (`localhost` server the user starts by hand). Removes the native-binary
  packaging problem but keeps a "get this running on your machine" step that a manager without
  developer tooling cannot clear alone.
- **Figma plugin bridge.** Rejected: Figma has no way to enforce the governed closed sets. It
  cannot know about `THEME_COLORS` tokens, per-locale `t("key")` budgets, or the no-overlap rule,
  so an export could look fine in Figma and fail compilation far from the surface the user was
  editing on — the worst possible feedback loop for a non-developer. Preview inside Figma is also
  never pixel-truth (a different renderer than the device), and it adds a proprietary cloud
  dependency to a project whose whole SOUP posture is minimizing exactly that kind of dependency.
- **VS Code extension / LSP.** Excellent for a developer, but it does not solve the actual
  problem: it is still a developer tool, so it fails the "manager without a developer" goal
  outright.

## Decision

Build **MedUI Studio** as a **hosted web service**: a manager opens a URL, no local install, no
git, no build toolchain.

```text
browser (palette + canvas + inspector, drag/drop, governed dropdowns)
        |  JSON / PNG over HTTP
tools/trustsc-medui-studio  (axum, host-only, never linked into runtime)
        |  in-process
crates/trustsc-ui-dsl-authoring   parse_medui_source / compile_medui_source
        |                          -> CompiledScreenSpec (nodes, bounds, golden refs)
        |                          <- serialize_screen (canonical .medui text)
adapters/trustsc-vulkan-winit     OffscreenRenderer (lavapipe) -> pixel-exact PNG
        |
"Propose change" -> branch + commit + PR  -> CI --verify-ui  (regulatory gate unchanged)
```

- **Server**: a new host-only tool crate, `tools/trustsc-medui-studio` (ADR-005 tools zone — axum
  and its tokio dependency tree never appear in `crates/` or `adapters/`, and this crate is never
  a dependency of anything shipped to a device).
- **Compiler reuse, no parallel implementation**: the server calls
  `trustsc-ui-dsl-authoring`'s parse/compile/serialize functions in-process — the same validation
  a CI build runs, so nothing the studio accepts can fail later in CI for a reason the studio
  didn't already show the user.
- **Pixel truth, one renderer**: preview frames are rendered by `adapters/trustsc-vulkan-winit`'s
  `OffscreenRenderer` on lavapipe (ADR-016) — the same renderer and same software rasterizer CI
  uses for verification, not a second implementation that could drift from what a device actually
  draws.
- **"Propose change" over direct commits**: saving in the studio never touches git directly. It
  produces canonical `.medui` text (via the serializer) and hands it to a "propose change" action
  that opens a branch, commits, and creates a pull request. The manager never touches git; the
  existing PR + `--verify-ui` CI gate (ADR-016) is completely unchanged and is not bypassed by
  this tool at any point.
- **Phasing**: a read-only previewer ships first (waves S6–S10) — immediately useful on its own
  and de-risks the render loop before any editing code exists — followed by the editor (waves
  S11–S15).

### Doctrine preserved

ADR-008 (deterministic `.medui` DSL boundary) and ADR-009 (compilation to generated artifacts)
are untouched. The studio round-trips `.medui` **source text**: parse → edit the AST → serialize
→ recompile, always through the same build-time-only compiler. There is no runtime DSL parsing
introduced anywhere, and no live-edit-to-device path — a device only ever runs a `.medui` file
that has been compiled the normal way, from source that landed via a normal PR.

### Foundation APIs authorized in the governed crate

This ADR authorizes the following additions to `crates/trustsc-ui-dsl-authoring` (no new
dependencies, **no serde** — JSON DTOs are a tool-zone concern, not a governed-crate one):

- A public AST (`ScreenDefinition` and friends) and `parse_medui_source(&str) -> Result<ScreenDefinition, Vec<Diagnostic>>`.
- A structured compile API, `compile_medui_source(...) -> Result<CompiledScreenSpec, Vec<Diagnostic>>`
  and `compile_screen_definition(...)`, exposing resolved nodes/bounds/golden references as data
  instead of generated Rust text.
- A canonical serializer, `serialize_screen(&ScreenDefinition) -> String`, so the studio's saved
  output is byte-for-byte the same style as hand-written `.medui` files.
- Palette catalog APIs (`widget_catalog`, `enumerate_text_keys`, `enumerate_numeric_templates`,
  `enumerate_images`) so the studio can populate governed dropdowns instead of accepting free text.

These four are deliberately form-factor-neutral: they are equally useful to a future CLI
(`medui-check`), an LSP, or any other tool, not just this studio.

### SOUP policy

axum and its tokio dependency tree, a `png` encoder/decoder for offscreen frame delivery,
`octocrab` (or equivalent) for the propose-change PR flow, and the frontend's own lockfile are
**tools-zone only** (ADR-005) and must be registered in `docs/governance/soup-register.toml`
before they are added in the waves that actually introduce them (S6 onward) — none are added by
this ADR itself.

### Known limits (v1)

- The serializer drops comments and blank lines: the parser strips trivia at parse time and the
  AST has no trivia slots to round-trip them. Accepted for v1; trivia preservation is a later
  wave if it turns out to matter in practice.
- Render latency is roughly 100–500 ms per frame on lavapipe, because each render currently
  creates a fresh Vulkan instance (`OffscreenRenderer` has no warm-instance reuse yet). Instance
  reuse across requests is a later optimization, not a blocker for the previewer milestone.
- Auth in v1 is a bearer token behind a reverse proxy — no user accounts or fine-grained
  permissions. Sufficient for a small internal manager audience; revisited if the studio grows an
  external audience.

## Consequences

- A non-developer can design or adjust a `.medui` screen end to end without installing anything,
  while every artifact that reaches a device still passes through the unchanged PR + CI gate.
- The governed crate gains public, data-oriented APIs (AST, compile, serialize, catalog) that
  outlive this specific tool and are reusable by any future `.medui` tooling.
- No parallel renderer or parallel validator is introduced: the studio's preview and validation
  are exactly the product's own compiler and offscreen render path, so nothing the studio shows
  can disagree with what CI verifies.
- New host-only dependencies (axum/tokio, `png`, `octocrab`, a frontend lockfile) are confined to
  `tools/trustsc-medui-studio` and must be tracked in the SOUP register when each wave introduces
  them; none of them ever reach `crates/` or `adapters/`.
- The editor (S11–S15) is deliberately deferred behind a working read-only previewer (S6–S10), so
  the higher-risk render/serve loop is proven before drag/resize/save code is written against it.

## References

- ADR-005 (pure-Rust project boundary and dependency policy) — the tools-zone boundary this
  server lives inside, and the SOUP register discipline for its new dependencies
- ADR-008 (deterministic MedUI DSL boundary) — the build-time-only doctrine this ADR reaffirms
- ADR-009 (MedUI compilation and generated artifacts) — the compiler this tool reuses in-process
- ADR-010 (MedUI i18n and text-budget policy) — the per-locale budgets the palette surfaces
- ADR-014 (precise positioning, image asset governance, theme colors) — the closed sets a
  governed dropdown enumerates
- ADR-016 (automated UI verification and manual generation) — `OffscreenRenderer` on lavapipe,
  reused here for pixel-exact preview, and the `--verify-ui` CI gate this tool's output still
  passes through unchanged
- docs/governance/soup-register.toml — where every new dependency this epic introduces is tracked
- Epic #9 — the wave breakdown (S1–S15) this ADR authorizes
