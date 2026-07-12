*This is a compact, complete index over `docs/iso14971/`'s modular files — one row per clause, not a
parallel transcription. It contains no reproduced standard text (see
[`../governance/citation-convention.md`](../governance/citation-convention.md)); use it to find which
module covers a given clause, then open that module for the actual explanatory prose and MduX-rust
cross-references.*

# ISO 14971:2019 — AI-Reference index

Every clause referenced by `docs/iso14971/01-*.md` through `04-*.md` is listed below, in clause
order, with a one-sentence pointer and a link to its detail. No clause is stubbed or condensed to a
placeholder — if a row exists here, its module has real content.

## §1-§4 — Scope and general requirements ([detail](01-scope-and-general-requirements.md))

- **§1 Scope** — applies to the manufacturer's device-level risk management, not to MduX-rust as an SDK. [→](01-scope-and-general-requirements.md#1-scope)
- **§2 Normative references** — ISO 13485 (the QMS it runs inside), IEC 62304 (software-specific risk analysis). [→](01-scope-and-general-requirements.md#2-normative-references)
- **§3 Terms and definitions** — hazard vs. hazardous situation; risk vs. residual risk. [→](01-scope-and-general-requirements.md#3-terms-and-definitions)
- **§4.1 Risk management process** — `ComplianceProgram` records process outputs; not an operating lifecycle process itself. [→](01-scope-and-general-requirements.md#41-risk-management-process)
- **§4.2 Management responsibilities** — entirely organizational; no MduX-rust scaffolding. [→](01-scope-and-general-requirements.md#42-management-responsibilities)
- **§4.3 Competence of personnel** — entirely the manufacturer's; no software-engineering component. [→](01-scope-and-general-requirements.md#43-competence-of-personnel)
- **§4.4 Risk management plan** — `ComplianceProgram::new(DeviceContext)` as a scope-bearing anchor only. [→](01-scope-and-general-requirements.md#44-risk-management-plan)
- **§4.5 Risk management file** — `trace_matrix_export()`/`audit_export()`/risk-record schema as candidate inputs, not the file itself. [→](01-scope-and-general-requirements.md#45-risk-management-file)

## §5-§6 — Risk analysis and evaluation ([detail](02-risk-analysis-and-evaluation.md))

- **§5.1 Risk analysis process** — no automated methodology; `Hazard`/risk-record schema record outputs. [→](02-risk-analysis-and-evaluation.md#51-risk-analysis-process)
- **§5.2 Intended use and reasonably foreseeable misuse** — a usability-engineering (IEC 62366-1) concern; no MduX-rust scaffolding. [→](02-risk-analysis-and-evaluation.md#52-intended-use-and-reasonably-foreseeable-misuse)
- **§5.3 Identification of characteristics related to safety** — `SafetyClass`/`DeterminismPolicy`, the SOUP register, and ADR-017's strict arithmetic as characterizing evidence. [→](02-risk-analysis-and-evaluation.md#53-identification-of-characteristics-related-to-safety)
- **§5.4 Identification of hazards and hazardous situations** — `Hazard.controlled_by`; risk-record's `hazard_ref`/`hazardous_situation` split; `class_c_monitor`'s sedation-alert example. [→](02-risk-analysis-and-evaluation.md#54-identification-of-hazards-and-hazardous-situations)
- **§5.5 Estimation of risk** — `severity`/`probability` enum fields; manufacturer's judgment, not computed. [→](02-risk-analysis-and-evaluation.md#55-estimation-of-the-risks-for-each-hazardous-situation)
- **§6 Risk evaluation** — `residual_risk_acceptable` boolean; `ComplianceProgram` has no acceptability gate of its own. [→](02-risk-analysis-and-evaluation.md#6-risk-evaluation)

## §7-§9 — Risk control and residual risk ([detail](03-risk-control-and-residual-risk.md))

- **§7.1 Risk reduction** — `Hazard.controlled_by`'s enforced link; option-preference order is the manufacturer's. [→](03-risk-control-and-residual-risk.md#71-risk-reduction)
- **§7.2 Risk control option analysis** — no automation; the ADR "alternatives considered" pattern as a loose analog. [→](03-risk-control-and-residual-risk.md#72-risk-control-option-analysis)
- **§7.3 Implementation of risk control measures** — ordinary requirement/verification machinery; `class_c_monitor`'s worked example. [→](03-risk-control-and-residual-risk.md#73-implementation-of-risk-control-measures)
- **§7.4 Residual risk evaluation** — `residual_risk_acceptable`/`risk_control_measures` fields; no computed value. [→](03-risk-control-and-residual-risk.md#74-residual-risk-evaluation)
- **§7.5 Risk-benefit analysis** — entirely clinical/regulatory judgment; no MduX-rust scaffolding. [→](03-risk-control-and-residual-risk.md#75-risk-benefit-analysis)
- **§7.6 Risks arising from risk control measures** — ADR-017's fail-closed self-test as a documented instance of this exact analysis. [→](03-risk-control-and-residual-risk.md#76-risks-arising-from-risk-control-measures)
- **§7.7 Completeness of risk control** — `Hazard::validate()`'s non-empty `controlled_by` as a partial, machine-checked floor. [→](03-risk-control-and-residual-risk.md#77-completeness-of-risk-control)
- **§8 Evaluation of overall residual risk** — `trace_rows()`/`release_evidence_summary()` as aggregate inputs; no risk-value aggregation performed. [→](03-risk-control-and-residual-risk.md#8-evaluation-of-overall-residual-risk)
- **§9 Risk management review** — `ComplianceProgram::validate()` and `AuditCategory::Release` as structured evidence; not the review itself. [→](03-risk-control-and-residual-risk.md#9-risk-management-review)

## §10 — Production and post-production activities ([detail](04-production-and-post-production.md))

- **§10.1 General** — no MduX-rust post-market presence; entirely the manufacturer's own system. [→](04-production-and-post-production.md#101-general)
- **§10.2 Collection of information** — `ProblemReport` is scoped to development/integration defects, not field reports. [→](04-production-and-post-production.md#102-collection-of-information)
- **§10.3 Review of experience** — `Hazard.controlled_by`/`VerificationCase.requirement` give a traceable path for re-review; the judgment itself is the manufacturer's. [→](04-production-and-post-production.md#103-review-of-experience)
- **§10.4 Actions** — re-enters modules 02-03's risk management process and IEC 62304's problem resolution process. [→](04-production-and-post-production.md#104-actions)

---

For a design decision's justification against a specific clause here, use the `Justification` object
in [`../iec62304/schemas/justification.schema.json`](../iec62304/schemas/justification.schema.json),
citing the clause with its exact key from this index.
