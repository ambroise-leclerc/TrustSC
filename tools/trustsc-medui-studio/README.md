# trustsc-medui-studio

The hosted MedUI Studio server (ADR-022, epic #9). Serves a browser frontend and a small JSON
API over an on-disk `.medui` repository checkout, so a manager or product owner can inspect (and,
in a later wave, edit) screens without a developer.

**Trust zone**: this is host tooling under `tools/` (ADR-005). It is never linked into any
`crates/` or `adapters/` code that ships to a device, and nothing here participates in the
build-time-only `.medui` compilation contract (ADR-008/009) beyond calling the same authoring
crate CI already uses.

## Running

```sh
cargo run -p trustsc-medui-studio -- --repo . --listen 127.0.0.1:8080
```

- `--repo <path>`: checkout containing `.medui` files to serve (default `.`).
- `--listen <addr:port>`: address to listen on (default `127.0.0.1:8080`).
- `--self-test`: compiles `examples/hello_world/hello_world.medui`, bridges it to the offscreen
  renderer, renders one frame, writes it to `./self-test-preview.png`, and exits nonzero on
  failure — without starting the server. Requires a Vulkan ICD (`sudo apt install
  mesa-vulkan-drivers` for lavapipe, the same setup CI uses).

## Render bridge (wave S7)

`src/render_bridge.rs` bridges an authoring-side `CompiledScreenSpec`
(`trustsc-ui-dsl-authoring`) to `adapters/trustsc-vulkan-winit`'s pixel-exact
`OffscreenRenderer` and encodes the captured frame as PNG — the exact same render path CI's
`--verify-ui` (ADR-016) exercises, so nothing the studio previews can disagree with what CI
verifies. `leak_package` does a mechanical `Box`/`String::leak` mapping from the authoring
spec's owned `String`s to the runtime `CompiledScreenPackage`'s `&'static str` fields (ADR-009);
each render leaks a few KB, acceptable for a host tool session and never done on a device.
Every dynamic realtime binding (numeric displays, streaming viewports, signal traces) is filled
with a placeholder value before rendering, so a preview never looks blank.

## Auth (v1)

If the `TRUSTSC_STUDIO_TOKEN` environment variable is set, every `/api/*` request must carry
`Authorization: Bearer <token>`; a missing or wrong token gets `401`. If the variable is unset or
empty, `/api/*` is unauthenticated.

This is a shared-secret bearer token only — there is no TLS termination, user accounts, or
per-user permission model in this crate. **TLS and any stronger authentication (SSO, mTLS, IP
allowlisting) are delegated to a reverse proxy** placed in front of this server; do not expose it
directly to an untrusted network.

## JSON DTOs (wave S8)

`src/dto.rs` mirrors every governed-crate type the API exposes as JSON: the `.medui` AST
(`ScreenDefinition` and everything it owns), compile diagnostics, the palette catalog, and a
compiled-node summary. `serde` derives and every DTO⇄governed-type conversion live here — the
governed crate itself (`trustsc-ui-dsl-authoring`) never depends on serde (S1's ADR). One naming
note if you're reading the JSON shapes: a `ScreenItemDto` (`Component` vs `Row`) is tagged
`"type"`, not `"kind"`, specifically because its `Component` payload already has its own `"kind"`
field (the widget kind) — reusing `"kind"` for both would collide when internally tagged.

## Endpoints

- `GET /healthz` — `200` with a version string.
- `GET /api/screens` — walks the configured repo for `**/*.medui` (skipping `target/` and dot
  directories), returning `[{ id, path, screen_name }]` where `screen_name` comes from parsing
  each file with `trustsc-ui-dsl-authoring::parse_medui_source` (the same parser CI uses).
- `GET /api/screens/{id}` — `{ source, screen, compiled: { surface, nodes, diagnostics } }` for
  the `.medui` file at `{id}` (the same `id`/`path` `GET /api/screens` returns). `screen` and
  `compiled.nodes` are `null`/empty if the source fails to parse or compile; `compiled.surface`
  falls back to `800x480` when the screen has no `surface:` pin.
- `POST /api/compile` — body `{ "source": "..." }` **or** `{ "screen": <AST DTO> }` (exactly one)
  → `{ ok, compiled, diagnostics }`. Never `500`s on bad input: a syntax or semantic error comes
  back as `ok: false` with `diagnostics: [{ message, line, severity }]`.
- `GET /api/frame?screen=<id>&locale=<tag>` and `POST /api/frame` (body like `/api/compile`, plus
  `locale`) → `image/png` at the screen's authored surface extent, rendered through the wave S7
  bridge. Renders are serialized through a single-slot semaphore (one Vulkan instance build at a
  time); an unknown locale is a `400`, a compile failure is a `422`.
- `GET /api/palette` — `{ widgets, colors, text_keys, templates, images, locales }`: the governed
  closed sets (`widget_catalog`, `THEME_COLORS`, `enumerate_text_keys`/`enumerate_numeric_templates`/
  `enumerate_images`) a governed dropdown UI needs, so it never has to accept free-typed values.
- `POST /api/serialize` — `{ "screen": <AST DTO> }` → `{ "source": "..." }` via `serialize_screen`.
  Rejects (`400`) a submitted `Panel` node — compiler-synthesized only, no `.medui` syntax exists
  for it — before ever reaching the serializer, which would otherwise panic on one.
- `GET /*` — serves the embedded `frontend/dist/` assets (the read-only previewer, wave S9).

Every `/api/*` route above is behind the same bearer-token gate (see Auth).

## Frontend (waves S9 + S11)

`frontend/` is a plain-TypeScript, no-framework, no-bundler app: a screen list and a screen view
(pixel-exact frame via `<img>`, a locale switcher, a zoom control, a node-bounds hover overlay, a
golden-reference-outline toggle, a diagnostics panel, and a PNG download button — wave S9), plus
canvas selection and drag/resize editing on top of it (wave S11).

Routing is a plain `location.hash` (`#screen=<id>&locale=<tag>`), so any screen+locale view is a
copy-pasteable, shareable URL — the server doesn't need a catch-all SPA route, since the hash
never reaches it.

### Canvas editor (wave S11)

`src/ast.ts` (pure AST helpers: find/update a node, grid-snap, "is this node draggable")
and `src/editor.ts` (`CanvasEditor` — all the DOM/interaction state: selection, drag/resize,
keyboard nudge, the context menu, the debounced compile loop) sit on top of `overlay.ts`'s
`renderOverlay`/`boundsToStyle`, which wave S9 deliberately factored out for exactly this.

- The client holds the AST DTO from `GET /api/screens/{id}` as the document; it's never persisted
  anywhere in this wave (no save/propose-change flow until wave S15) — reload or navigate away
  and an edit is gone.
- Only absolutely-positioned, fixed-px nodes (`position:` + fixed `width:`/`height:`) can be
  dragged/resized — the parser requires that pairing anyway (`parse_component_properties`). Flow
  nodes (no `position:`, or `Fill` dims) show a "flow" badge and offer "Convert to absolute" in a
  right-click context menu, which pins their *current compiled* bounds as `position:` + fixed
  dims — visually lossless.
- Drag moves only the overlay rect (no compile/render per mouse move); on drop — or debounced
  ~250ms after a keyboard nudge (arrows = 1px, Shift+arrows = 8px) — the AST is sent to
  `POST /api/compile`. On success, a fresh frame is requested via `POST /api/frame` (as a blob,
  swapped into the `<img>` via an object URL) and the overlay geometry updates immediately —
  it doesn't wait on that frame request, since overlay positions come from the compile result,
  not the image itself. On failure the last-good frame and compiled bounds are kept, the edited
  node's *proposed* bounds are drawn as a red outline, and the diagnostics panel shows why.
- Clicking a Row's synthesized background (a compiled `Panel` node, id `{row_id}-background`)
  selects the Row instead of trying to treat it as a draggable widget — inspector-only until
  wave S13.

```sh
cd frontend
npm ci
npm run build   # tsc -p tsconfig.json, then copies styles.css and index.html into dist/
```

`frontend/dist/` (the build output `include_dir!` embeds into the binary, wave S6) is committed
alongside the source, so `cargo build`/`cargo run` never requires Node — only touch the frontend
if you're changing it, then rebuild `dist/` and commit both. `frontend/package-lock.json` is
committed and `typescript` is tracked in `docs/governance/soup-register.toml`.

See `MANUAL_TESTS.md` for the browser checklist both waves' acceptance criteria require (Rust
tests only cover the API and static-asset serving, not actual rendering/interaction in a
browser).

## Deployment (wave S10)

`Dockerfile` is a multi-stage build: a `rust:1-bookworm` build stage, then a `debian:bookworm-
slim` runtime stage with `mesa-vulkan-drivers` (lavapipe, ADR-016 §8 — the same software Vulkan
rasterizer CI uses) and `libvulkan1` (the loader `ash`'s `loaded` feature `dlopen`s at runtime;
the build stage needs no Vulkan dev headers at all, since nothing links against it at compile
time). Build from the **repository root** — this crate is a Cargo workspace member, so the whole
workspace is the build context:

```sh
docker build -f tools/trustsc-medui-studio/Dockerfile -t trustsc-medui-studio .
```

Two deployment modes, both driven by `entrypoint.sh`:

- **Bind-mounted checkout** (the simple case — an existing `.medui` repo checkout on the host):
  ```sh
  docker run -p 8080:8080 \
    -e TRUSTSC_STUDIO_TOKEN=<token> \
    -v /path/to/checkout:/data:ro \
    trustsc-medui-studio
  ```
- **Cloned at container startup** (`STUDIO_REPO_URL` set to a git remote; useful when the studio
  runs somewhere the host checkout isn't available). The clone lands in `$STUDIO_REPO_DIR`
  (default `/data`) and is reused — not re-cloned — on container restart as long as that path is
  a persistent volume:
  ```sh
  docker run -p 8080:8080 \
    -e TRUSTSC_STUDIO_TOKEN=<token> \
    -e STUDIO_REPO_URL=https://github.com/<org>/<repo>.git \
    -v studio-data:/data \
    trustsc-medui-studio
  ```

`docker-compose.yml` in this directory is a runnable example of the bind-mounted mode:

```sh
TRUSTSC_STUDIO_TOKEN=<token> docker compose -f tools/trustsc-medui-studio/docker-compose.yml up
```

Both modes were verified end to end in a container with **no host GPU**: `/healthz` and
`/api/frame` (a real, non-blank PNG via lavapipe) both work purely on CPU software rendering.

### Deployment posture

This image has no TLS termination and only the v1 shared-bearer-token auth described above under
"Auth" — **put a reverse proxy in front of it** for TLS and any stronger authentication (SSO,
mTLS, IP allowlisting); never expose the container's port directly to an untrusted network. The
container's own auth is a floor, not a complete access-control story: treat
`TRUSTSC_STUDIO_TOKEN` as a shared secret with the same handling discipline as any other
credential (a secrets manager or orchestrator-native secret, not a value baked into an image or
committed anywhere).

### CI smoke (`.github/workflows/ci.yml`)

After the lavapipe install step, CI runs `cargo run -p trustsc-medui-studio -- --self-test`
(wave S7's render-bridge self-test: compiles `hello_world.medui`, bridges it, renders one frame,
and fails the build if the frame doesn't match the authored extent or comes back blank) — the
same lavapipe-backed check this Dockerfile's runtime image relies on, run directly on the runner
rather than through a Docker build for CI turnaround time.
