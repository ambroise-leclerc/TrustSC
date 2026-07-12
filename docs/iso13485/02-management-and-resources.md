# ISO 13485: Management responsibility and resource management

## Module overview

§5 and §6 describe how an organization's leadership commits to, plans, and resources its QMS —
these clauses are almost entirely about organizational governance (policy, roles, review meetings,
staffing, facilities) rather than engineering artifacts, so this module is more explicit than most
about where TrustSC simply has no analogue and the manufacturer's own QMS carries the whole
obligation. Where a real, checkable connection exists — quality objectives that resemble
requirement traceability, competence records for reviewers of safety-critical code — it is pointed
out; where none exists, that is stated plainly rather than stretched.

**Key areas covered:**
- Management commitment, customer focus, quality policy, and QMS planning
- Responsibility, authority, communication, and management review
- Provision of resources and human resource competence
- Infrastructure, work environment, and contamination control

---

## §5 Management responsibility

### §5.1 Management commitment

Top management must provide evidence of commitment to developing and implementing the QMS and
maintaining its effectiveness: communicating the importance of meeting customer and regulatory
requirements, establishing the quality policy and objectives, conducting management review, and
ensuring resource availability.

This is an organizational-leadership obligation with no software analogue — TrustSC cannot
provide "evidence of management commitment" for an organization it has no relationship with. The
closest indirect signal available to an organization evaluating whether to adopt TrustSC is the
project's own engineering discipline as demonstrated in its ADR trail and CI gating (every ADR is
`Status: Accepted`, per `docs/adr/README.md`, and `.github/workflows/ci.yml` runs the full build,
test, and evidence-verification suite on every push) — but that is evidence of *this project's*
engineering rigor, not of the adopting organization's management commitment, and should not be
conflated with it.

### §5.2 Customer focus

Top management must ensure customer requirements and applicable regulatory requirements are
determined and met. For a medical device manufacturer, "customer requirements" include clinical and
usability needs, and "applicable regulatory requirements" include the device's classification and
jurisdiction-specific rules.

TrustSC's contribution here is narrow but real: `trustsc_governance::Requirement { id, title,
source_clause, verification_intent }` gives a manufacturer's software requirements — which are
themselves usually downstream of customer/regulatory requirements determined at a higher level — a
structured, traceable representation, and the `source_clause` field is designed to hold exactly the
kind of citation this corpus uses (`ISO 13485:2016 §5.2 Customer focus`, or more often a downstream
IEC 62304/ISO 14971 clause) so a requirement's regulatory origin stays attached to it rather than
living only in a separate spreadsheet.

### §5.3 Quality policy

Top management must ensure the quality policy is appropriate to the organization's purpose,
includes a commitment to comply with requirements and maintain QMS effectiveness, provides a
framework for quality objectives, and is communicated, understood, and reviewed for continuing
suitability throughout the organization.

Entirely an organizational document; TrustSC has no quality policy of its own to point to, and a
project README or CLAUDE.md file is not a substitute for one, regardless of how much engineering
rigor either document describes.

### §5.4 Planning

#### §5.4.1 Quality objectives

Top management must ensure quality objectives, including those needed to meet product requirements,
are established at relevant functions and levels, and are measurable and consistent with the
quality policy.

`ComplianceProgram::release_evidence_summary()` (`crates/trustsc-governance/src/lib.rs`) — a one-line
snapshot of `device=... class=... requirements=... hazards=... verifications=... problems=...
audit_events=...` — is the kind of measurable output an organization's own quality objectives could
reference (for example, "zero open problem reports at release," which is directly readable from
this summary's `problems=` count), but the objectives themselves, their measurability criteria, and
their review are the organization's to set, not TrustSC's to supply.

#### §5.4.2 Quality management system planning

QMS planning must ensure the processes needed for the QMS, and for meeting quality objectives, are
identified, and that QMS integrity is maintained when changes are planned and implemented.

No TrustSC analogue at the QMS-planning level. The closest process-level parallel within this
project's own scope is how a new ADR is expected to interact with existing ones — `docs/adr/README.md`
requires reading prior ADRs before proposing a change that crosses an existing architectural
boundary, which is a narrow instance of "maintain system integrity when a change is planned," scoped
to software architecture rather than the QMS as a whole.

### §5.5 Responsibility, authority and communication

#### §5.5.1-§5.5.2 Responsibility, authority, and the management representative

Top management must ensure responsibilities and authorities are defined, documented, and
communicated, and must appoint a member of management (the "management representative") with
authority and responsibility for ensuring QMS processes are established/implemented/maintained,
reporting on QMS performance and improvement needs, and promoting awareness of applicable
regulatory and QMS requirements throughout the organization.

Organizational roles with no engineering analogue. TrustSC's own repository has commit review and
an ADR-acceptance gate, but a GitHub repository's contribution rules are not a management
representative appointment, and this corpus does not claim otherwise.

#### §5.5.3 Internal communication

The organization must ensure appropriate communication processes are established and that
communication takes place regarding QMS effectiveness. `trustsc_governance::AuditEvent`'s `Lifecycle`
category (recorded via `ComplianceProgram::record_event`) is, at most, a machine-readable trail of
*what changed in the governance data* that a manufacturer's internal communication process could
surface to relevant staff — it is not itself an internal communication process.

### §5.6 Management review

#### §5.6.1-§5.6.3 General, review input, and review output

Top management must review the QMS at planned intervals to ensure its continuing suitability,
adequacy, and effectiveness, using a defined set of inputs (audit results, feedback, process
performance, product conformity, corrective/preventive actions, prior review follow-up, changes
affecting the QMS, and improvement recommendations) and producing outputs feeding continual
improvement, product changes needed to meet requirements, and resource needs.

`ComplianceProgram::trace_matrix_export()` and `audit_export()` are candidate *inputs* to a
management review that covers the software slice of a device built on TrustSC — they give a
reviewer a structured, current view of requirement/verification/hazard coverage and the sequence of
governance-data changes since the last review, rather than a static document that drifts out of
date. They are inputs only: TrustSC performs no review itself, sets no review interval, and
produces no review output or minutes. That is the organization's management review process to run.

## §6 Resource management

### §6.1 Provision of resources

The organization must determine and provide the resources needed to implement and maintain the QMS
and its effectiveness, and to meet applicable regulatory and customer requirements. Purely an
organizational resourcing decision — outside TrustSC's scope entirely.

### §6.2 Human resources

Personnel performing work affecting product quality must be competent based on appropriate
education, training, skills, and experience; the organization must determine necessary competence,
provide training or other action to achieve it, evaluate effectiveness, ensure personnel are aware
of the relevance of their activities and how they contribute to quality objectives, and maintain
appropriate records of education/training/skills/experience.

TrustSC's contribution is indirect but genuine: the trust-zone architecture (ADR-005) narrows
*which* personnel competence matters most for reviewing the software item itself. Because governed
crates (`crates/`) are `#![forbid(unsafe_code)]` pure Rust with `unsafe`, native SDK bindings, and
FFI confined entirely to `adapters/`, the competence a reviewer needs to review the governed core
safely (safe-Rust reasoning) is narrower than the competence needed to review, say, the Vulkan
adapter or a host-side baker tool — a manufacturer's training-needs analysis under §6.2 can use this
boundary to scope reviewer qualifications per component rather than requiring every reviewer to be
competent across the whole native-binding surface. This is a scoping aid, not a substitute for the
organization's own competence records.

### §6.3 Infrastructure

The organization must determine, provide, and maintain the infrastructure needed for product
conformity, including buildings/workspace/utilities, process equipment (hardware and software), and
supporting services (transport, communication, information systems), with documented requirements
where infrastructure activities including maintenance could affect product quality.

TrustSC is process equipment (software) in this sub-clause's sense when a manufacturer builds it
into their toolchain. `CLAUDE.md`'s "Replaying CI locally" section, and `.github/workflows/ci.yml`
itself, are the closest thing TrustSC offers to a documented, reproducible build infrastructure
specification: pinned dependencies (`Cargo.lock`, `--locked` builds), a named Vulkan software
rasterizer for CI (`lavapipe`, ADR-016 §8), and named prerequisite system packages
(`libvulkan1 libvulkan-dev vulkan-tools`) that a manufacturer's own infrastructure-maintenance
record could reference directly rather than re-deriving from scratch.

### §6.4 Work environment and contamination control

#### §6.4.1 Work environment

The organization must determine and manage the work environment needed to achieve product
conformity, including documenting requirements if work environment conditions could adversely
affect product quality, and health/cleanliness/clothing requirements for personnel in contact with
product or work environment where applicable.

#### §6.4.2 Contamination control

Where required (implanted or sterile devices, and processes where contamination control is
necessary), the organization must plan and document arrangements for contamination control of
product and the work environment.

Neither sub-clause has any TrustSC analogue: this is a software library with no physical work
environment, personnel health/clothing requirements, or contamination exposure of its own. A
manufacturer producing a device with physical components alongside TrustSC-based software applies
§6.4 entirely to that physical production environment, independent of anything in this repository.

---

## Related documents

- [Foundations and quality management system](01-foundations-and-qms.md)
- [Product realisation](03-product-realisation.md)
- [Measurement, analysis and improvement](04-measurement-analysis-improvement.md)
- [AI-Reference index](AI-Reference.md)
- [Citation convention](../governance/citation-convention.md)
