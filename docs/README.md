# Documentation

Complete, English-language documentation for MduX-rust. Start with the
[README](../README.en.md) (or [French version](../README.md)) for the project pitch and a
quickstart; the pages below go deeper.

- **[Getting started](getting-started.md)** — full walkthroughs of `examples/hello_world` and
  `examples/class_c_monitor` (the NeuroSense 500), Vulkan prerequisites, and the complete command
  reference.
- **[Architecture](architecture.md)** — the governed/adapter/tools trust-zone boundary, the crate
  map, the evidence-generation pattern, CI, and asset governance.
- **[Regulatory compliance](regulatory-compliance.md)** — how MduX-rust's engineering practices
  are designed to align with IEC 62304 Class B/C, how its evidence artifacts and governance types
  are meant to feed a manufacturer's technical file and notified-body audits, and — just as
  importantly — what it explicitly does *not* provide.
- **[Architecture decision records](adr/README.md)** — all 18 accepted ADRs, indexed with a
  one-line summary each.
- **[MedUI DSL reference](dsl/overview.md)** — the `.medui` build-time UI description language:
  [language reference](dsl/language-reference.md), [component dictionary](dsl/component-dictionary.md),
  [safety-monitor contract](dsl/safety-monitor-contract.md), [build integration](dsl/build-integration.md).
- **[SOUP register](governance/soup-register.toml)** — the structured, audit-ready register of
  every third-party dependency reachable from host tooling and presentation adapters.

Known gap: this repository does not yet have a `CONTRIBUTING.md`. If you're looking to
contribute, open an issue first to confirm scope and approach.
