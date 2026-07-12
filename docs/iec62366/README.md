# IEC 62366-1:2015+AMD1:2020 — Usability engineering for medical devices

This folder is an LLM- and human-navigable breakdown of IEC 62366-1, MduX-rust's usability
engineering standard, written as original explanatory prose against the standard's real clause
structure — see [`../governance/citation-convention.md`](../governance/citation-convention.md) for
why no normative standard text is reproduced here, and for the citation-key format used throughout
(`IEC 62366-1:2015 §N.M Clause title`).

IEC 62366-1 governs the process a manufacturer runs to make a device's user interface safe and
effective to use — it is normatively linked to ISO 14971 (a hazard-related use scenario is a
software/UI-contributed risk) and referenced by IEC 62304 §2 for any device with a user interface.
MduX-rust's MedUI DSL and `--verify-ui` tooling provide structural, compile-time and inspection-based
support for a narrow slice of this process; this corpus is explicit throughout about which
sub-clauses that support covers and which remain entirely the manufacturer's responsibility (use
specification, formative/summative testing with real users).

## Modules

| Module | Clauses | Focus |
|---|---|---|
| [01-scope-and-general-requirements.md](01-scope-and-general-requirements.md) | §1-§4 | Scope, definitions, application of the standard, the usability engineering file |
| [02-use-specification-and-ui-design.md](02-use-specification-and-ui-design.md) | §5.1-§5.5 | Use specification, hazard-related use scenarios, UI specification/evaluation-plan/design |
| [03-formative-and-summative-evaluation.md](03-formative-and-summative-evaluation.md) | §5.6-§5.7 | Formative and summative evaluation |

## Other resources

- [`AI-Reference.md`](AI-Reference.md) — one-line-per-clause index across the whole standard.
- [`schemas/usability-engineering-record.schema.json`](schemas/usability-engineering-record.schema.json) —
  JSON Schema for one use-scenario/evaluation entry, documentation-level (no `mdux-governance` Rust
  type mirrors it yet).
- [`../governance/citation-convention.md`](../governance/citation-convention.md) — citation format
  and the `Justification` object shared by all five standards in this corpus.
- [`../regulatory-compliance.md`](../regulatory-compliance.md) — how this corpus fits into the
  project's overall regulatory-compliance story and its stated scope limits.
- [`../../software_development_file/regulatory/IEC_62366/`](../../software_development_file/regulatory/IEC_62366/) —
  MduX-rust's own filled-in Usability Engineering File, which cites into this corpus.

## For AI agents

When generating or reviewing code, an ADR, or an SDF document that claims IEC 62366-1 alignment: find
the relevant module above by clause range, cite the clause using the exact citation-key format, and
be explicit about the boundary between what MduX-rust's MedUI DSL/`--verify-ui` mechanically enforces
(structure, text-budget compliance, rendered-bounds containment) and what remains a manufacturer's
own clinical/human-factors judgment (use specification, hazard-related-scenario identification,
formative/summative evaluation with real users) — every module in this folder draws that line
explicitly and is the pattern to follow.
