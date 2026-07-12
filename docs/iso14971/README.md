# ISO 14971:2019 — Application of risk management to medical devices

This folder is an LLM- and human-navigable breakdown of ISO 14971, TrustSC's general risk
management standard, written as original explanatory prose against the standard's real clause
structure — see [`../governance/citation-convention.md`](../governance/citation-convention.md) for
why no normative standard text is reproduced here, and for the citation-key format used throughout
(`ISO 14971:2019 §N.M Clause title`).

This corpus is the *sister* document to
[`docs/iec62304/06-risk-management-process.md`](../iec62304/06-risk-management-process.md): that
module already covers the software-specific slice of risk management (how software contributes to
hazardous situations, software risk control measures, their verification); this whole folder covers
the general risk-management process that slice sits inside — hazard/hazardous-situation
identification, risk estimation and evaluation, risk control, residual risk, and post-production
surveillance. Every module here cites into the IEC 62304 module rather than duplicating it.

## Modules

| Module | Clauses | Focus |
|---|---|---|
| [01-scope-and-general-requirements.md](01-scope-and-general-requirements.md) | §1-§4 | Scope, definitions, establishing a risk management process/plan/file |
| [02-risk-analysis-and-evaluation.md](02-risk-analysis-and-evaluation.md) | §5-§6 | Risk analysis (intended use, hazard/hazardous-situation identification, risk estimation), risk evaluation |
| [03-risk-control-and-residual-risk.md](03-risk-control-and-residual-risk.md) | §7-§9 | Risk control, evaluation of overall residual risk, risk management review |
| [04-production-and-post-production.md](04-production-and-post-production.md) | §10 | Collection and review of production/post-production information, resulting actions |

## Safety/risk record quick reference

`trustsc_governance::Hazard` (`crates/trustsc-governance/src/lib.rs`) models a hazard's identity and its
controlling requirement(s); [`schemas/risk-record.schema.json`](schemas/risk-record.schema.json)
adds the per-hazardous-situation detail (severity, probability, residual-risk acceptance) ISO 14971
distinguishes but `Hazard` alone does not model — see
[02-risk-analysis-and-evaluation.md §5.4](02-risk-analysis-and-evaluation.md#54-identification-of-hazards-and-hazardous-situations).

## Other resources

- [`AI-Reference.md`](AI-Reference.md) — one-line-per-clause index across the whole standard.
- [`schemas/risk-record.schema.json`](schemas/risk-record.schema.json) — JSON Schema for a
  per-hazardous-situation risk record, cross-referencing `docs/iec62304/schemas/hazard.schema.json`.
- [`../governance/citation-convention.md`](../governance/citation-convention.md) — citation format
  and the `Justification` object shared by all five standards in this corpus.
- [`../regulatory-compliance.md`](../regulatory-compliance.md) — how this corpus fits into the
  project's overall regulatory-compliance story and its stated scope limits.
- `software_development_file/regulatory/ISO_14971/Risk_Management_File.md` (added in a later PR in
  this stack) — TrustSC's own filled-in risk management file, which cites into this corpus.

## For AI agents

When generating or reviewing code, an ADR, or an SDF document that claims ISO 14971 alignment: find
the relevant module above by clause range, cite the clause using the exact citation-key format, and
be explicit that TrustSC performs no part of hazard identification, risk estimation, or
risk-acceptability judgment itself — it supplies typed places (`Hazard`, the risk-record schema) to
*record* the outcome of a manufacturer's own analysis and clinical/engineering judgment, not the
analysis itself. Every module in this folder draws that line explicitly and is the pattern to follow.
