# Documentation

Complete, English-language documentation for TrustSC. Start with the
[README](../README.en.md) (or [French version](../README.md)) for the project pitch and a
quickstart; the pages below go deeper.

- **[Getting started](getting-started.md)** — full walkthroughs of `examples/hello_world` and
  `examples/class_c_monitor` (the NeuroSense 500), Vulkan prerequisites, and the complete command
  reference.
- **[Architecture](architecture.md)** — the governed/adapter/tools trust-zone boundary, the crate
  map, the evidence-generation pattern, CI, and asset governance.
- **[Regulatory compliance](regulatory-compliance.md)** — how TrustSC's engineering practices
  are designed to align with IEC 62304 Class B/C, how its evidence artifacts and governance types
  are meant to feed a manufacturer's technical file and notified-body audits, and — just as
  importantly — what it explicitly does *not* provide.
- **[Architecture decision records](adr/README.md)** — all 20 accepted ADRs, indexed with a
  one-line summary each.
- **[MedUI DSL reference](dsl/overview.md)** — the `.medui` build-time UI description language:
  [language reference](dsl/language-reference.md), [component dictionary](dsl/component-dictionary.md),
  [safety-monitor contract](dsl/safety-monitor-contract.md), [build integration](dsl/build-integration.md).
- **[SOUP register](governance/soup-register.toml)** — the structured, audit-ready register of
  every third-party dependency reachable from host tooling and presentation adapters.
- **[AI agent onboarding](../AGENTS.md)** — the canonical, tool-neutral instruction file for AI
  coding agents (trust zones, commands, the regulatory citation protocol), with task-scoped
  skills under `.claude/skills/` and the MCP policy in
  [ADR-020](adr/ADR-020-ai-interaction-standardization.md). Citation keys and `Justification`
  blocks across `docs/` and `software_development_file/` are machine-checked in CI by
  `tools/trustsc-docs-lint`.

## Regulatory standards reference

An LLM- and human-navigable corpus of original explanatory prose (never reproduced standard text —
see [`governance/citation-convention.md`](governance/citation-convention.md)) for the five standards
TrustSC's compliance scaffolding targets, each broken into modules by clause range with a compact
`AI-Reference.md` index and JSON Schemas ([ADR-019](adr/ADR-019-regulatory-standards-reference-corpus.md)):

- **[IEC 62304](iec62304/README.md)** — software life cycle processes (the flagship, most-cited corpus).
- **[ISO 13485](iso13485/README.md)** — quality management systems.
- **[ISO 14971](iso14971/README.md)** — application of risk management to medical devices.
- **[IEC 62366-1](iec62366/README.md)** — usability engineering.
- **[IEC 81001-5-1](iec81001/README.md)** — health software cybersecurity in the product life cycle
  (clause numbering here is a best-effort placeholder — see that folder's opening caveat).

## Software development file

**[`software_development_file/`](../software_development_file/README.md)** — a `templates/` tree any
manufacturer building on TrustSC can fill in, and a `regulatory/` tree with the same documents
filled in for TrustSC itself (Software Architecture/Design Description, SOUP list, Usability
Engineering File, Cybersecurity SAD, QMS scope note, Risk Management File), citing into the
regulatory standards reference above.

Known gap: this repository does not yet have a `CONTRIBUTING.md`. If you're looking to
contribute, open an issue first to confirm scope and approach.
