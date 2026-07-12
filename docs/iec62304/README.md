# IEC 62304:2006+AMD1:2015 — Medical device software life cycle processes

This folder is an LLM- and human-navigable breakdown of IEC 62304, TrustSC's primary software
lifecycle standard, written as original explanatory prose against the standard's real clause
structure — see [`../governance/citation-convention.md`](../governance/citation-convention.md) for
why no normative standard text is reproduced here, and for the citation-key format used throughout
(`IEC 62304:2006 §N.M Clause title`).

## Modules

| Module | Clauses | Focus |
|---|---|---|
| [01-scope-and-general-requirements.md](01-scope-and-general-requirements.md) | §1-§4 | Scope, definitions, QMS/risk-management integration, safety classification, legacy software |
| [02-development-planning-and-requirements.md](02-development-planning-and-requirements.md) | §5.1-§5.2 | Development planning, requirements analysis |
| [03-development-design.md](03-development-design.md) | §5.3-§5.4 | Architectural and detailed design, segregation, SOUP identification |
| [04-development-implementation-and-testing.md](04-development-implementation-and-testing.md) | §5.5-§5.8 | Unit implementation/verification, integration testing, system testing, release |
| [05-maintenance-process.md](05-maintenance-process.md) | §6 | Maintenance plan, problem/modification analysis, modification implementation |
| [06-risk-management-process.md](06-risk-management-process.md) | §7 | Software-specific hazard analysis, risk control measures, verification |
| [07-configuration-management-process.md](07-configuration-management-process.md) | §8 | Configuration identification, change control, status accounting, SOUP |
| [08-problem-resolution-process.md](08-problem-resolution-process.md) | §9 | Problem report handling, evaluation, resolution traceability |

## Safety classification quick reference

`trustsc_core::SafetyClass` (`crates/trustsc-core/src/lib.rs`) models **Class B and Class C only** — see
[01-scope-and-general-requirements.md §4.3](01-scope-and-general-requirements.md#43-software-safety-classification).
Nothing in this corpus or in TrustSC implies Class A support.

## Other resources

- [`AI-Reference.md`](AI-Reference.md) — one-line-per-clause index across the whole standard, for
  quickly locating which module covers a given clause.
- [`schemas/`](schemas/) — JSON Schemas for `Requirement`, `Hazard`, `VerificationCase`, a safety
  classification record, and the shared `Justification` object, field-aligned with
  `crates/trustsc-governance/src/lib.rs`.
- [`../governance/citation-convention.md`](../governance/citation-convention.md) — citation format
  and the `Justification` object shared by all five standards in this corpus.
- [`../regulatory-compliance.md`](../regulatory-compliance.md) — how this corpus fits into the
  project's overall regulatory-compliance story and its stated scope limits.
- `software_development_file/regulatory/IEC_62304/` (added in a later PR in this stack) —
  TrustSC's own filled-in SAD/SDD/SOUP documents, which cite into this corpus.

## For AI agents

When generating or reviewing code, an ADR, or an SDF document that claims IEC 62304 alignment: find
the relevant module above by clause range, cite the clause using the exact citation-key format, and
prefer pointing at a real TrustSC mechanism (a type, an ADR, a CI step) over restating the
standard's requirement in the abstract — every module in this folder does this throughout and is the
pattern to follow.
