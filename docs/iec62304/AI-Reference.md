*This is a compact, complete index over `docs/iec62304/`'s modular files — one row per clause, not a
parallel transcription. It contains no reproduced standard text (see
[`../governance/citation-convention.md`](../governance/citation-convention.md)); use it to find which
module covers a given clause, then open that module for the actual explanatory prose and MduX-rust
cross-references.*

# IEC 62304:2006+AMD1:2015 — AI-Reference index

Every clause referenced by `docs/iec62304/01-*.md` through `08-*.md` is listed below, in clause
order, with a one-sentence pointer and a link to its detail. No clause is stubbed or condensed to a
placeholder — if a row exists here, its module has real content.

## §1-§4 — Scope and general requirements ([detail](01-scope-and-general-requirements.md))

- **§1 Scope** — applies to development/maintenance of medical device software; MduX-rust is an SDK a
  manufacturer's device software uses, not itself the regulated device. [→](01-scope-and-general-requirements.md#1-scope)
- **§2 Normative references** — ties to ISO 13485, ISO 14971, IEC 62366-1 (and informatively IEC 81001-5-1). [→](01-scope-and-general-requirements.md#2-normative-references)
- **§3 Terms and definitions** — SOUP and the software item/unit/system hierarchy matter most here. [→](01-scope-and-general-requirements.md#3-terms-and-definitions)
- **§4.1 Quality management system** — MduX-rust doesn't provide the QMS; see `docs/iso13485/`. [→](01-scope-and-general-requirements.md#41-quality-management-system)
- **§4.2 Risk management process** — `Hazard::validate()`'s non-empty `controlled_by` is the machine-checked integration point. [→](01-scope-and-general-requirements.md#42-risk-management-process)
- **§4.3 Software safety classification** — Class A/B/C defined; `SafetyClass` models only B/C. [→](01-scope-and-general-requirements.md#43-software-safety-classification)
- **§4.4 Legacy software** — not currently applicable to MduX-rust itself. [→](01-scope-and-general-requirements.md#44-legacy-software)

## §5.1-§5.2 — Planning and requirements ([detail](02-development-planning-and-requirements.md))

- **§5.1.1-§5.1.3 Software development planning** — plan content, currency; ADR trail + CI + `docs/architecture.md` as the plan's components. [→](02-development-planning-and-requirements.md#51-software-development-planning)
- **§5.2.1 Define requirements** — `Requirement { id, title, source_clause, verification_intent }`. [→](02-development-planning-and-requirements.md#521-defining-requirements)
- **§5.2.2 Include risk control measures** — `Hazard.controlled_by` links a control back to a requirement. [→](02-development-planning-and-requirements.md#522-include-risk-control-measures-in-requirements)
- **§5.2.3 Re-evaluation and update** — `AuditEvent`/`add_requirement` sequencing. [→](02-development-planning-and-requirements.md#523-requirements-re-evaluation-and-update)
- **§5.2.4 Verify requirements** — every `Requirement` needs ≥1 `VerificationCase` (`ComplianceProgram::validate()`). [→](02-development-planning-and-requirements.md#524-verify-software-requirements)
- **§5.2.5 Requirements approval** — ADR-011 intends `@safety_critical` MedUI nodes to bind a requirement, but the compiler doesn't enforce it yet. [→](02-development-planning-and-requirements.md#525-requirements-approval)

## §5.3-§5.4 — Design ([detail](03-development-design.md))

- **§5.3.1-§5.3.2 Architecture and interfaces** — the `crates/`/`adapters/`/`tools/` trust-zone split (ADR-005) and crate-map interfaces. [→](03-development-design.md#53-software-architectural-design)
- **§5.3.3 Segregation for risk control** — `#![forbid(unsafe_code)]` makes segregation compiler-enforced. [→](03-development-design.md#533-identify-segregation-necessary-for-risk-control)
- **§5.3.4 SOUP identification** — `docs/governance/soup-register.toml`. [→](03-development-design.md#534-identify-soup-items)
- **§5.3.5 Verify architectural design** — the ADR review process. [→](03-development-design.md#535-verify-the-architectural-design)
- **§5.4.1-§5.4.3 Detailed design** — per-crate module structure and interface-level public APIs, verified by `cargo test`. [→](03-development-design.md#54-software-detailed-design)

## §5.5-§5.8 — Implementation through release ([detail](04-development-implementation-and-testing.md))

- **§5.5.1-§5.5.3 Unit implementation and verification** — coding standards, `cargo test`, `Classifier1D::new()`'s runtime self-test, bake/`verify` artifact checks. [→](04-development-implementation-and-testing.md#55-software-unit-implementation-and-verification)
- **§5.6.1-§5.6.2 Integration and integration testing** — `FrameworkBuilder`'s composition and cross-validation. [→](04-development-implementation-and-testing.md#56-software-integration-and-integration-testing)
- **§5.7.1-§5.7.2 System testing** — `--verify-ui`'s `GoldenBounds`/`InkContainment` checks. [→](04-development-implementation-and-testing.md#57-software-system-testing)
- **§5.8.1-§5.8.4 Release** — `ComplianceProgram::validate()`, `ProblemReport.closed`, `compliance_label()`, `release_evidence_summary()`. [→](04-development-implementation-and-testing.md#58-software-release)

## §6 — Maintenance process ([detail](05-maintenance-process.md))

- **§6.1 Maintenance plan** — CI + locked dependencies + bake/verify as scaffolding, not a full plan. [→](05-maintenance-process.md#61-establish-software-maintenance-plan)
- **§6.2.1-§6.2.2 Problem/modification analysis** — `ProblemReport`, and re-entering risk management on affecting changes. [→](05-maintenance-process.md#62-problem-and-modification-analysis)
- **§6.3.1 Modification implementation** — modifications flow through the same development-process modules. [→](05-maintenance-process.md#63-modification-implementation)

## §7 — Risk management process ([detail](06-risk-management-process.md))

- **§7.1.1-§7.1.3 Analysis of contributing hazards** — `Hazard`; Class C requires ≥1 recorded hazard. [→](06-risk-management-process.md#71-analysis-of-software-contributing-to-hazardous-situations)
- **§7.2.1-§7.2.2 Risk control measures** — identified as `Hazard.controlled_by` requirements; `class_c_monitor`'s alert path as a worked example. [→](06-risk-management-process.md#72-risk-control-measures)
- **§7.3.1-§7.3.2 Verification of risk control measures** — same requirement-verification rule; fail-closed self-test as a "no new hazard" design choice. [→](06-risk-management-process.md#73-verification-of-risk-control-measures)
- **§7.4 Risk management of software changes** — connects to §6.2.2; `Verification`-category `AuditEvent`s. [→](06-risk-management-process.md#74-risk-management-of-software-changes)

## §8 — Configuration management process ([detail](07-configuration-management-process.md))

- **§8.1.1-§8.1.3 Configuration identification** — `Cargo.lock` + `--locked` builds; SOUP register entries. [→](07-configuration-management-process.md#81-configuration-identification)
- **§8.2.1-§8.2.3 Change control** — byte-verified bake/`verify` evidence artifacts (ADR-007). [→](07-configuration-management-process.md#82-change-control)
- **§8.3.1 Status accounting records** — `AuditEvent`'s `Lifecycle` category. [→](07-configuration-management-process.md#831-maintain-records-of-configuration-item-history)
- **§8.3.2 SOUP anomaly list** — partially covered by `soup-register.toml`'s `risk_controls`; flagged as a gap for the manufacturer to close. [→](07-configuration-management-process.md#832-soup-anomaly-list)

## §9 — Problem resolution process ([detail](08-problem-resolution-process.md))

- **§9.1.1-§9.1.5 Establish problem resolution process** — `ProblemReport`, its audit trail, and the trend-analysis gap. [→](08-problem-resolution-process.md#91-establish-problem-resolution-process)
- **§9.2.1-§9.2.7 Evaluation and traceability** — resolution via modules 02-04; `soup-register.toml`'s `component_id` as the SOUP-trend join key. [→](08-problem-resolution-process.md#92-software-problem-resolution-process-evaluation-and-traceability)

---

For a design decision's justification against a specific clause here, use the `Justification` object
in [`schemas/justification.schema.json`](schemas/justification.schema.json), citing the clause with
its exact key from this index.
