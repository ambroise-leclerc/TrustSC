*This is a compact, complete index over `docs/iso13485/`'s modular files — one row per clause, not a
parallel transcription. It contains no reproduced standard text (see
[`../governance/citation-convention.md`](../governance/citation-convention.md)); use it to find which
module covers a given clause, then open that module for the actual explanatory prose and MduX-rust
cross-references.*

# ISO 13485:2016 — AI-Reference index

Every clause referenced by `docs/iso13485/01-*.md` through `04-*.md` is listed below, in clause
order, with a one-sentence pointer and a link to its detail. No clause is stubbed or condensed to a
placeholder — if a row exists here, its module has real content.

## §1-§4 — Foundations and QMS ([detail](01-foundations-and-qms.md))

- **§1 Scope** — applies to the organization/manufacturer, not to MduX-rust as an SDK. [→](01-foundations-and-qms.md#1-scope)
- **§2 Normative references** — ISO 9000 vocabulary; ISO 14971 risk management integration throughout. [→](01-foundations-and-qms.md#2-normative-references)
- **§3 Terms and definitions** — medical device file; advisory notice/complaint/nonconforming product. [→](01-foundations-and-qms.md#3-terms-and-definitions)
- **§4.1 General requirements** — QMS process identification; outsourced-process control maps onto MduX-rust's trust-zone architecture and SOUP register. [→](01-foundations-and-qms.md#41-general-requirements)
- **§4.2.1 General documentation** — the ADR trail as design-rationale evidence, not a quality manual. [→](01-foundations-and-qms.md#421-general)
- **§4.2.2 Quality manual** — no MduX-rust analogue; entirely manufacturer-authored. [→](01-foundations-and-qms.md#422-quality-manual)
- **§4.2.3 Medical device file** — `ComplianceProgram` exports as one input among many. [→](01-foundations-and-qms.md#423-medical-device-file)
- **§4.2.4 Control of documents** — `Cargo.lock`/`--locked` builds and bake/`verify` as a code/asset analogue. [→](01-foundations-and-qms.md#424-control-of-documents)
- **§4.2.5 Control of records** — `AuditEvent`/`audit_export()`, with no persistence layer of its own. [→](01-foundations-and-qms.md#425-control-of-records)

## §5-§6 — Management and resources ([detail](02-management-and-resources.md))

- **§5.1 Management commitment** — organizational leadership obligation; no software analogue. [→](02-management-and-resources.md#51-management-commitment)
- **§5.2 Customer focus** — `Requirement.source_clause` traces a requirement back to its regulatory/customer origin. [→](02-management-and-resources.md#52-customer-focus)
- **§5.3 Quality policy** — entirely organizational; no MduX-rust analogue. [→](02-management-and-resources.md#53-quality-policy)
- **§5.4.1 Quality objectives** — `release_evidence_summary()` as a measurable output an objective could reference. [→](02-management-and-resources.md#541-quality-objectives)
- **§5.4.2 QMS planning** — the ADR-precedence-review convention as a narrow architectural parallel. [→](02-management-and-resources.md#542-quality-management-system-planning)
- **§5.5.1-§5.5.2 Responsibility/authority/management representative** — organizational roles; no engineering analogue. [→](02-management-and-resources.md#551-552-responsibility-authority-and-the-management-representative)
- **§5.5.3 Internal communication** — `AuditEvent`'s `Lifecycle` category as a machine-readable change trail only. [→](02-management-and-resources.md#553-internal-communication)
- **§5.6 Management review** — `trace_matrix_export()`/`audit_export()` as candidate review inputs, not the review itself. [→](02-management-and-resources.md#56-management-review)
- **§6.1 Provision of resources** — purely organizational; out of scope. [→](02-management-and-resources.md#61-provision-of-resources)
- **§6.2 Human resources** — the trust-zone boundary scopes reviewer-competence needs per component. [→](02-management-and-resources.md#62-human-resources)
- **§6.3 Infrastructure** — CI workflow and pinned dependencies as a build-infrastructure specification. [→](02-management-and-resources.md#63-infrastructure)
- **§6.4 Work environment/contamination control** — no MduX-rust analogue; physical-production scope only. [→](02-management-and-resources.md#64-work-environment-and-contamination-control)

## §7 — Product realisation ([detail](03-product-realisation.md))

- **§7.1 Planning of product realization** — `DeviceContext`/`ComplianceProgram`/`UiSdkConfig` choices, cross-validated by `FrameworkBuilder::build()`. [→](03-product-realisation.md#71-planning-of-product-realization)
- **§7.2.1-§7.2.3 Customer-related processes** — manufacturer sales/contract process; `source_clause` as the one loosely-related traceability landing point. [→](03-product-realisation.md#72-customer-related-processes)
- **§7.3 Design and development planning** — the software-item slice of IEC 62304 §5.1's planning applied at product level. [→](03-product-realisation.md#design-and-development-planning)
- **§7.3 Design and development inputs** — `Requirement`/`Hazard.controlled_by` as structured design-input records. [→](03-product-realisation.md#design-and-development-inputs)
- **§7.3 Design and development outputs** — compiled `CompiledScreenPackage`/`ModelPackage` as versioned, acceptance-checked outputs. [→](03-product-realisation.md#design-and-development-outputs)
- **§7.3 Design and development review** — the ADR acceptance process as a narrow architectural-review analogue. [→](03-product-realisation.md#design-and-development-review)
- **§7.3 Design and development verification** — every `Requirement` needs ≥1 `VerificationCase`; `VerificationMethod`'s closed enum. [→](03-product-realisation.md#design-and-development-verification)
- **§7.3 Design and development validation** — `--verify-ui` as engineering-level validation, not clinical validation. [→](03-product-realisation.md#design-and-development-validation)
- **§7.3 Design and development transfer** — CI's build/test/verify pipeline as the software-transfer analogue. [→](03-product-realisation.md#design-and-development-transfer)
- **§7.3 Control of design and development changes** — `AuditEvent` sequencing and ADR supersession as a recorded change trail. [→](03-product-realisation.md#control-of-design-and-development-changes)
- **§7.3 Design and development files** — `trace_matrix_export()`/`audit_export()` as exportable slices of the file. [→](03-product-realisation.md#design-and-development-files)
- **§7.4 Purchasing** — the SOUP register as a dependency/supplier evaluation record. [→](03-product-realisation.md#74-purchasing)
- **§7.5.1-§7.5.11 Production and service provision** — physical manufacturing scope; `compliance_label()` as a minimal traceability string for §7.5.9. [→](03-product-realisation.md#75-production-and-service-provision)
- **§7.6 Control of monitoring and measuring equipment** — `--verify-ui` and baker `verify` subcommands as re-checked measurement tooling; toolchain-pinning gap noted. [→](03-product-realisation.md#76-control-of-monitoring-and-measuring-equipment)

## §8 — Measurement, analysis and improvement ([detail](04-measurement-analysis-improvement.md))

- **§8.1 General** — `ComplianceProgram::validate()`/`release_evidence_summary()` as one narrow measurement method. [→](04-measurement-analysis-improvement.md#81-general)
- **§8.2.1 Feedback** — no MduX-rust analogue; no post-market presence. [→](04-measurement-analysis-improvement.md#821-feedback)
- **§8.2.2 Complaint handling** — `ProblemReport` is deliberately not a complaint-handling record. [→](04-measurement-analysis-improvement.md#822-complaint-handling)
- **§8.2.3 Reporting to regulatory authorities** — no MduX-rust analogue whatsoever. [→](04-measurement-analysis-improvement.md#823-reporting-to-regulatory-authorities)
- **§8.2.4 Internal audit** — trace-matrix/audit exports as audit evidence; `AuditEvent`'s name is not §8.2.4 coverage. [→](04-measurement-analysis-improvement.md#824-internal-audit)
- **§8.2.5 Monitoring/measurement of processes** — CI's continuous baker-verify/`--verify-ui` checks, scoped to two processes only. [→](04-measurement-analysis-improvement.md#825-monitoring-and-measurement-of-processes)
- **§8.2.6 Monitoring/measurement of product** — `ComplianceProgram::validate()` as a release gate matching this sub-clause's principle. [→](04-measurement-analysis-improvement.md#826-monitoring-and-measurement-of-product)
- **§8.3.1-§8.3.4 Control of nonconforming product** — no dedicated type; `validate()` failure is the nearest adjacent (negative) mechanism. [→](04-measurement-analysis-improvement.md#831-834-general-pre-delivery-actions-post-delivery-actions-and-rework)
- **§8.4 Analysis of data** — raw material exposed, no aggregation/trend analysis performed. [→](04-measurement-analysis-improvement.md#84-analysis-of-data)
- **§8.5.1-§8.5.3 Improvement / CAPA** — `ProblemReport`/`Hazard.controlled_by`/`VerificationCase.requirement` give traceability; root-cause/CAPA judgment is entirely the manufacturer's. [→](04-measurement-analysis-improvement.md#85-improvement)

---

For a design decision's justification against a specific clause here, use the `Justification` object
in [`../iec62304/schemas/justification.schema.json`](../iec62304/schemas/justification.schema.json),
citing the clause with its exact key from this index.
