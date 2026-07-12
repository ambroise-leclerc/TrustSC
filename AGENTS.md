# AGENTS.md

Instructions for AI coding agents (and a fast onboarding page for humans) working in this
repository. This file is the canonical, tool-neutral entry point; deeper material lives in
`docs/` and is linked from here rather than duplicated.

## What this project is

TrustSC is a pure-Rust medical-device UI SDK with IEC 62304 Class B / Class C compliance
modeling built in, targeting Vulkan (Class B) and Vulkan SC (Class C). Everything about its
architecture is shaped by one idea: the code that runs on a device must be deterministic,
allocation-free, `unsafe`-free, and auditable, while everything complicated (font shaping, shader
compilation, ML model preparation, UI-DSL compilation) happens offline on the host and ships as
byte-verified, committed evidence artifacts.

It is a sibling of — not part of — the C++ projects under `Projets_MduX/` (`MduX`, `SpecLab`,
`mddlog`); nothing in their documentation applies here.

## Trust zones (read this before adding any dependency)

The workspace splits into three zones ([ADR-005](docs/adr/ADR-005-pure-rust-project-boundary-and-dependency-policy.md),
[ADR-012](docs/adr/ADR-012-presentation-adapter-crates-and-shader-artifacts.md)):

| Zone | Directory | Rules |
|---|---|---|
| Governed | `crates/` | Pure Rust, `#![forbid(unsafe_code)]`, depend only on each other or version-pinned reviewed crates. No FFI types, native handles, or bindgen output in public APIs. |
| Edge adapters | `adapters/` | May use `unsafe` and native SDK bindings (`ash`, `winit`, ...) internally, but every public function takes/returns owned Rust data defined by a governed crate. `examples/` are adapters in this sense too. |
| Host tooling | `tools/` | Bakers that produce generated evidence artifacts. May use additional reviewed third-party crates. Never linked into device/runtime crates or shipped in runtime artifacts. |

Before adding a dependency, determine which zone the consuming crate lives in — that decides
whether the dependency is even permissible without a new ADR. Every third-party crate reachable
from `tools/` or `adapters/` must have an entry in
[`docs/governance/soup-register.toml`](docs/governance/soup-register.toml).

## Commands

```bash
source $HOME/.cargo/env

cargo build --locked --workspace        # build everything
cargo test --locked --quiet             # run all tests
cargo test <test_name>                  # run a single test

# examples (windowed paths need a system Vulkan loader; --headless-smoke does not)
cargo run -p hello_world -- --auto-close-ms=1000
cargo run -p class_c_monitor            # NeuroSense 500 demo (ML inference + waveform)
```

### Replaying CI locally

`.github/workflows/ci.yml` is the canonical command set. Reproduce it with:

```bash
cargo build --locked --workspace
cargo test --locked --quiet
cargo run --locked -q -p trustsc-docs-lint -- check
cargo run --locked -q -p trustsc-font-baker -- verify tools/trustsc-font-baker/fixtures/roboto-demo.toml generated/fonts/roboto-regular-16px/package.json generated/fonts/roboto-regular-16px/report.json
cargo run --locked -q -p trustsc-font-baker -- verify tools/trustsc-font-baker/fixtures/roboto-display-48px.toml generated/fonts/roboto-display-48px/package.json generated/fonts/roboto-display-48px/report.json
cargo run --locked -q -p trustsc-font-baker -- verify tools/trustsc-font-baker/fixtures/roboto-display-160px.toml generated/fonts/roboto-display-160px/package.json generated/fonts/roboto-display-160px/report.json
cargo run --locked -q -p trustsc-image-baker -- verify tools/trustsc-image-baker/fixtures/acme-logo.toml generated/images/acme-logo/package.json generated/images/acme-logo/report.json
cargo run --locked -q -p trustsc-shader-baker -- verify tools/trustsc-shader-baker/fixtures/text-shaders.toml adapters/trustsc-vulkan-winit/shaders/generated adapters/trustsc-vulkan-winit/shaders/generated/report.json
cargo run --locked -q -p trustsc-ml-baker -- verify tools/trustsc-ml-baker/fixtures/eeg-demo.toml generated/models/eeg-demo/package.json generated/models/eeg-demo/report.json
cargo run --locked -q -p hello_world -- --headless-smoke
cargo run --locked -q -p class_c_monitor -- --headless-smoke
cargo run --locked -q -p hello_world -- --verify-ui=generated/verification --locales=en-US
cargo run --locked -q -p class_c_monitor -- --verify-ui=generated/verification --locales=all
```

## The regulatory reference corpus — how to use `docs/`

`docs/` contains an LLM- and human-navigable corpus of **original explanatory prose** for the
five standards this project's compliance scaffolding targets
([ADR-019](docs/adr/ADR-019-regulatory-standards-reference-corpus.md)):
[IEC 62304](docs/iec62304/README.md), [ISO 13485](docs/iso13485/README.md),
[ISO 14971](docs/iso14971/README.md), [IEC 62366-1](docs/iec62366/README.md),
[IEC 81001-5-1](docs/iec81001/README.md). Each standard folder holds clause-range modules
(`NN-*.md`), a compact one-row-per-clause `AI-Reference.md` index, and JSON Schemas aligned with
`crates/trustsc-governance`.

**Protocol — follow this whenever code, an ADR, or a `software_development_file/` document claims
alignment with a standard:**

1. Open that standard's `docs/<standard>/README.md` and find the module covering the clause range;
   use `AI-Reference.md` for fast clause lookup.
2. Cite clauses with the exact citation-key format from
   [`docs/governance/citation-convention.md`](docs/governance/citation-convention.md):
   `<Standard> §<clause> <Short clause title>`, e.g.
   `IEC 62304:2006 §5.2 Software development planning`. The only valid standard identifiers are
   `IEC 62304:2006`, `ISO 13485:2016`, `ISO 14971:2019`, `IEC 62366-1:2015`, `IEC 81001-5-1:2021`.
   The same string is used verbatim as a module heading, an `AI-Reference.md` row, and any
   `clause_ref` field.
3. Where a design decision needs a formal link to a clause, emit a `Justification` object
   (fenced `json` block) per
   [`docs/iec62304/schemas/justification.schema.json`](docs/iec62304/schemas/justification.schema.json):
   `justification_id` (`JUS-NNN`, unique across the whole corpus), `standard`, `clause_ref`,
   `rationale`, optional `requirement_id`, `evidence_refs[]` (real repo paths). See
   `software_development_file/regulatory/IEC_62304/SAD.md` for worked examples.
4. **Never reproduce or closely paraphrase a standard's normative text.** IEC/ISO standards are
   copyrighted; the corpus is original prose written against clause numbers/titles for exactly
   this reason. Cite the clause and explain in your own words.
5. IEC 81001-5-1 clause numbering in this corpus is provisional — see the caveat at the top of
   `docs/iec81001/README.md` before relying on its clause references.

Citation keys and `Justification` blocks are machine-checked in CI by
`cargo run --locked -q -p trustsc-docs-lint -- check`; run it after editing anything under `docs/`
or `software_development_file/`.

`software_development_file/` has two mirrored trees: `templates/` (blank documents any
manufacturer building on TrustSC fills in) and `regulatory/` (the same documents filled in for
TrustSC itself). Filled documents summarize — never duplicate — machine registers like the SOUP
register.

## Coding and artifact rules

- Governed crates keep `#![forbid(unsafe_code)]`. If a change seems to need `unsafe` or a native
  handle in `crates/`, it belongs in an edge adapter instead, or needs a new ADR.
- `Cargo.lock` is committed and CI builds with `--locked`; update the lockfile deliberately.
- Everything under `generated/` (and `adapters/trustsc-vulkan-winit/shaders/generated/`) is
  deterministic build **evidence** — regenerate via the matching baker in `tools/`
  (`bake` to produce, `verify` to check); never hand-edit it.
- `.medui` UI screens are compiled at build time only ([docs/dsl/](docs/dsl/overview.md)); no DSL
  parsing, layout solving, or text shaping ever happens on-device.
- ADRs under [`docs/adr/`](docs/adr/README.md) are the authoritative source for *why* each
  boundary exists — read the relevant one before proposing a change that crosses the
  governed/adapter/tools line or alters a compile-time-only contract, and write a new ADR when a
  change moves such a boundary.
- Naming: `UpperCamelCase` types, `snake_case` functions/variables/modules, crate names prefixed
  `trustsc-`.

## Git conventions

- Commit messages are plain and descriptive. **Never add AI attribution of any kind** — no
  `Co-Authored-By` trailers, no "Generated with ..." footers or links, in commits or PR
  descriptions.
- Large features land as small stacked PRs (see the ADR trail and merged PR history for the
  pattern), each leaving CI green.

## AI tooling

- **Skills** — `.claude/skills/` holds task-specific playbooks in the open Agent Skills
  `SKILL.md` format (regulatory citations, evidence pipeline, MedUI authoring, SDF documents).
  Tools that support the format load them automatically; any other agent or human can read them
  as ordinary markdown.
- **MCP** — no MCP servers are adopted for this repository; the evaluation and rationale live in
  [ADR-020](docs/adr/ADR-020-ai-interaction-standardization.md).
