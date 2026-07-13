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

## Auth (v1)

If the `TRUSTSC_STUDIO_TOKEN` environment variable is set, every `/api/*` request must carry
`Authorization: Bearer <token>`; a missing or wrong token gets `401`. If the variable is unset or
empty, `/api/*` is unauthenticated.

This is a shared-secret bearer token only — there is no TLS termination, user accounts, or
per-user permission model in this crate. **TLS and any stronger authentication (SSO, mTLS, IP
allowlisting) are delegated to a reverse proxy** placed in front of this server; do not expose it
directly to an untrusted network.

## Endpoints (this wave)

- `GET /healthz` — `200` with a version string.
- `GET /api/screens` — walks the configured repo for `**/*.medui` (skipping `target/` and dot
  directories), returning `[{ id, path, screen_name }]` where `screen_name` comes from parsing
  each file with `trustsc-ui-dsl-authoring::parse_medui_source` (the same parser CI uses).
- `GET /*` — serves the embedded `frontend/dist/` assets (a placeholder page for this wave; the
  read-only previewer lands in wave S9).
