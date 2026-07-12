---
name: evidence-pipeline
description: Regenerate or verify the deterministic evidence artifacts under generated/ and adapters/trustsc-vulkan-winit/shaders/generated/ (fonts, images, shaders, ML model packages). Use when a change touches any baked artifact, a baker fixture/recipe, a shader source, a vendored asset, or when CI's verify steps fail.
---

# Evidence pipeline

Every generated artifact in this repo follows the ADR-007 pattern: a host-only baker in `tools/`
consumes a committed recipe (`tools/<baker>/fixtures/*.toml`), deterministically produces the
artifact plus a `report.json`, and CI re-verifies the committed bytes on every push.

**Never hand-edit anything under `generated/` or `adapters/trustsc-vulkan-winit/shaders/generated/`**
— always regenerate through the baker, then commit the changed bytes together with the recipe
change that caused them.

## The four bakers

| Baker | Recipe fixtures | Output |
|---|---|---|
| `trustsc-font-baker` | `tools/trustsc-font-baker/fixtures/*.toml` | `generated/fonts/<id>/{package.json,report.json}` |
| `trustsc-image-baker` | `tools/trustsc-image-baker/fixtures/*.toml` | `generated/images/<id>/{package.json,report.json}` |
| `trustsc-shader-baker` | `tools/trustsc-shader-baker/fixtures/text-shaders.toml` | `adapters/trustsc-vulkan-winit/shaders/generated/*.spv` + `report.json` |
| `trustsc-ml-baker` | `tools/trustsc-ml-baker/fixtures/*.toml` | `generated/models/<id>/{package.json,report.json}` |

Two subcommands each:
- `bake <recipe> <out...>` — regenerate the artifact (run after changing a recipe, GLSL source,
  vendored font/image, or model definition).
- `verify <recipe> <artifact> <report>` — byte-verify the committed artifact (what CI runs; see
  the exact argument lists in `.github/workflows/ci.yml` or the CI-replay block in `AGENTS.md`).

`trustsc-ml-baker` also has `import` (pulls Hugging Face `safetensors` into a recipe): it is an
offline, host-only authoring aid — it never runs in CI and its output recipe must be
human-reviewed before committing.

## Source assets vs. evidence

- `assets/` (e.g. `assets/fonts/roboto/`) holds approved **source** assets with provenance
  manifests (`font-manifest.toml`) — changing one requires updating its manifest.
- `generated/` holds **derived** evidence — reproducible from sources + recipes, but committed so
  CI can prove nothing drifted.
- `generated/verification/` is the exception: `--verify-ui` output, regenerated fresh each run,
  gitignored, uploaded as a CI artifact only.

## After any bake

```bash
cargo test --locked --quiet                     # runtimes self-validate packages at load
cargo run --locked -q -p <baker> -- verify ...  # confirm the committed bytes verify
```

Then run the relevant example with `--headless-smoke` if the artifact feeds one. Bakers and their
dependencies are SOUP-registered host tooling (`docs/governance/soup-register.toml`) and must
never be linked into `crates/` or shipped in runtime artifacts.
