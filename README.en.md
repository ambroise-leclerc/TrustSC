# MduX-rust

🇫🇷 [Version française](README.md)

**A pure-Rust framework for building medical-device software aligned with the requirements of
IEC 62304 (software life-cycle processes), ISO 13485 (quality management system), and ISO 14971
(risk management).** It provides directly reusable Class B/C building blocks — a Vulkan (Class B)
and Vulkan SC (Class C) UI, zero-SOUP on-device AI inference — and, more broadly, evidence
generation designed to feed a manufacturer's own QMS and the technical file submitted to a
notified body.

## The Class B/C challenge

Teams building IEC 62304 Class B or Class C software run into the same friction over and over:
requirement-to-verification traceability maintained by hand and drifting from the code; a
third-party dependency surface (SOUP) that grows fastest exactly in the UI and AI/ML layers most
visible to an operator; evidence an auditor can't easily reproduce; and safety-critical UI
elements whose behavior is hard to guarantee once the rendering stack allocates or shapes text at
runtime.

## What MduX-rust provides today

MduX-rust splits the workspace into three trust zones — a small, `unsafe`-free governed core
(`crates/`), edge adapters that isolate native Vulkan/windowing bindings (`adapters/`), and
host-only tooling that never ships in a runtime artifact (`tools/`) — so review effort
concentrates where it matters. Every asset pipeline (fonts, images, shaders, and now ML weights)
bakes a source input into committed, byte-verified evidence (`package.json` + `report.json`),
re-checked automatically in CI instead of asserted by hand. On top of that, `mdux-governance`
provides working `Requirement`/`Hazard`/`VerificationCase`/`AuditEvent` types with structured
trace-matrix and audit-trail export.

The flagship example of this approach is the ML pipeline: an on-device classifier
(`Classifier1D`) written entirely in `#![forbid(unsafe_code)]` Rust — no ONNX Runtime, no
PyTorch — whose weights are baked, versioned data. Swapping a Hugging Face demonstrator model for
a manufacturer's own clinically-qualified weights changes zero lines of inference or application
code, and the engine fails closed at startup if its own golden self-test doesn't reproduce
bit-for-bit. See `examples/class_c_monitor`, the Acme NeuroSense 500 depth-of-anesthesia monitor,
for the full, working demonstration.

This is a framework and a set of compliance APIs — not a certified medical device, and not a
substitute for a manufacturer's own engineering judgment.

## Notified bodies and audits

For a notified-body reviewer, the trust-zone split means deep code review can concentrate on a
small governed core instead of the entire dependency graph; generated evidence artifacts carry
their own SHA-256 digest and are byte-verified in CI rather than re-audited by hand each release;
the SOUP register (`docs/governance/soup-register.toml`) already has the shape — supplier,
license, integration path, risk controls — a technical file's SOUP section asks for; and 19
accepted ADRs document the design rationale behind every boundary. None of this replaces a
manufacturer's own QMS, risk file, or notified-body engagement — see
**[Regulatory compliance](docs/regulatory-compliance.md)** for the full treatment, including an
explicit list of what this project does and does not provide.

## Regulatory reference corpus and software development file

The two efforts previously tracked here as a roadmap are now delivered
([ADR-019](docs/adr/ADR-019-regulatory-standards-reference-corpus.md)):

- **Standards references usable by developers' LLMs** — `docs/iec62304/`, `docs/iso13485/`,
  `docs/iso14971/`, `docs/iec62366/`, and `docs/iec81001/` each break their standard into modules
  by clause range, with a compact `AI-Reference.md` index and JSON Schemas per standard. Unlike
  the framework's original C++ project (`MduX`), whose "AI Reference" docs paraphrased the actual
  standard text closely enough to raise a real copyright concern, this corpus contains **original
  explanatory prose only** — every clause is cited by number and title, never quoted — and drops
  that project's redundant third "Framework" tier: this page, the ADR trail, and
  `software_development_file/regulatory/` already are the "applied to this project" layer.
- **Regulatory documentation templates** — [`software_development_file/`](software_development_file/README.md)
  has a `templates/` tree any manufacturer fills in, and a `regulatory/` tree with the same
  documents filled in for MduX-rust itself, citing real ADRs, `mdux-governance` types, and
  examples.

Details and tracking: **[Regulatory compliance](docs/regulatory-compliance.md)**.

## Quickstart

```bash
source $HOME/.cargo/env

cargo build                                  # build everything
cargo test                                   # run all tests
cargo run -p hello_world                     # smallest example (opens a Vulkan window)
cargo run -p hello_world -- --headless-smoke # no window, no Vulkan — for CI
cargo run -p class_c_monitor                 # NeuroSense 500: 3D UI + zero-SOUP ML
```

Full command reference and Vulkan installation steps: **[Getting started](docs/getting-started.md)**.

## Workspace structure

| Directory | Contents |
|---|---|
| `crates/` | Governed core: device/compliance model, UI policy, text and ML pipelines, the `mdux` facade. |
| `adapters/mdux-vulkan-winit` | The Vulkan + winit presentation adapter — the only crate touching native windowing/graphics bindings. |
| `tools/` | Host-only bake/verify tooling for fonts, images, shaders, and ML model evidence. |
| `examples/` | `hello_world` (smallest smoke demo), `class_b_device`, `class_c_monitor` (NeuroSense 500), `class_c_vulkansc_device`. |

Full crate-by-crate map and the trust-zone rationale: **[Architecture](docs/architecture.md)**.

## Vulkan prerequisites

```bash
# Ubuntu / Debian
sudo apt-get install libvulkan1 libvulkan-dev vulkan-tools

# macOS
brew install vulkan-loader molten-vk vulkan-tools
```

Only needed for the windowed path — `--headless-smoke` runs without a Vulkan loader. Full
platform setup: **[Getting started](docs/getting-started.md#vulkan-prerequisites)**.

## Full documentation

- **[Documentation home](docs/README.md)**
- **[Regulatory compliance](docs/regulatory-compliance.md)** — IEC 62304, notified bodies, the
  evidence pattern, the regulatory standards corpus and SDF tree, and honest scope boundaries.
- **[Architecture](docs/architecture.md)** — trust zones, crate map, CI, asset governance.
- **[Getting started](docs/getting-started.md)** — full example walkthroughs and command reference.
- **[Architecture decision records](docs/adr/README.md)** — all 19 accepted ADRs.
- **[MedUI DSL reference](docs/dsl/overview.md)** — the `.medui` build-time UI language.

## License

To be finalized.
