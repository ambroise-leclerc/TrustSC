# ISO 13485: Foundations and quality management system

## Module overview

This module covers ISO 13485:2016's front matter — scope, normative references, and the terms and
definitions that structure everything downstream — plus §4, the clause that establishes the
quality management system (QMS) itself: what a QMS must cover in general, and what it must document
in particular (the quality manual, the medical device file, document control, and record control).
Clauses 5 through 8, which describe how the QMS is *run* rather than *established*, are covered by
modules 02-04.

**Key areas covered:**
- Scope and applicability to organizations in the medical device supply chain
- Terms and definitions load-bearing for the rest of the standard
- General QMS requirements, including outsourced-process control
- Documentation requirements: quality manual, medical device file, document control, record control

---

## §1 Scope

ISO 13485:2016 specifies requirements for a quality management system used by an organization
involved in one or more stages of a medical device's life cycle — design and development,
production, storage and distribution, installation, servicing, and final decommissioning/disposal
— or in the supply of related activities and services (such as technical support). Unlike ISO 9001,
it is a *regulatory-compliance* standard first: its primary purpose is to support organizations in
demonstrating conformity to applicable regulatory requirements, not customer satisfaction as a
standalone goal.

MduX-rust does not fall inside this scope directly. It is a software development kit consumed by a
manufacturer's own design and development activities; it is not itself an organization, and it does
not perform production, distribution, installation, or servicing of a device. §1's scope applies to
the *manufacturer* who integrates MduX-rust into their device software and operates a QMS around
that integration — see `docs/regulatory-compliance.md`'s "Purpose and scope" section, which states
this project provides "engineering scaffolding," not a QMS in its own right. Everything in this
corpus should be read as: which pieces of a manufacturer's §4-§8 obligations does a mechanism in
MduX-rust give evidence toward, and which remain entirely the manufacturer's to build.

## §2 Normative references

ISO 13485:2016 is normatively tied to ISO 9000 (quality management vocabulary) for shared
terminology, and references ISO 14971 (risk management) throughout as the standard a conformant QMS
must integrate risk management against — most visibly in the design and development clause (module
03 of this corpus, `docs/iso13485/03-product-realisation.md`) and in corrective/preventive action
(module 04). It does not reference IEC 62304 or IEC 62366-1 directly (those are horizontal software
and usability standards a device manufacturer applies *within* the QMS this standard establishes),
but in practice a Class B/C software manufacturer's QMS procedures for design and development are
where ISO 13485 §7.3 and IEC 62304 §5 meet — see `docs/iec62304/README.md` and this corpus's module
03 for where that overlap is real rather than asserted.

## §3 Terms and definitions

Two definitions matter most for how this corpus and MduX-rust's governance model line up:

- **Medical device file** — the standard's term for the compiled set of records demonstrating
  conformity to this standard and to applicable regulatory requirements for a particular device
  type (see §4.2.3 below). It is a broader record than an IEC 62304 software development file, but
  the two overlap heavily for a software-only or software-dominant device: `mdux_governance::ComplianceProgram`'s
  structured exports (`trace_matrix_export()`, `audit_export()`, `release_evidence_summary()`,
  `crates/mdux-governance/src/lib.rs`) are the kind of software-side content a medical device file
  would incorporate for a device built on MduX-rust, not a substitute for the file itself.
- **Advisory notice, complaint, nonconforming product** — terms that drive clause 8 (module 04).
  MduX-rust has no post-market presence of its own (it is a library, not a shipped device), so these
  definitions apply entirely to the manufacturer's finished product, not to anything this project
  tracks directly — a point worth stating plainly rather than stretching `mdux_governance::ProblemReport`
  to cover a role it doesn't fill (it is closer to an IEC 62304 §9 problem report than to a §8.2.2
  regulatory complaint record; see module 04 §8.2.2 below for the distinction).

## §4 Quality management system

### §4.1 General requirements

An organization must establish, document, implement, and maintain a QMS and continually maintain
its effectiveness, including identifying the processes needed, their sequence and interaction, the
criteria and methods needed to ensure their effective operation and control, and — where the
organization chooses to outsource a process affecting product conformity — retaining
responsibility for that outsourced process and documenting how it is controlled.

This last point is where MduX-rust's trust-zone architecture is directly relevant, even though
MduX-rust is not itself a QMS. A manufacturer who builds a device UI or ML inference path on
MduX-rust is, in effect, relying on externally-developed software components for parts of their
design and development process — precisely the situation §4.1's outsourced-process language
addresses. `docs/architecture.md` and ADR-005 (pure-Rust project boundary and dependency policy)
give that manufacturer a documented answer to "how is this outsourced/incorporated software
controlled": the governed crates (`crates/`) are `#![forbid(unsafe_code)]` pure Rust with a
minimal, reviewable dependency surface; the SOUP register
(`docs/governance/soup-register.toml`) lists every third-party dependency with its supplier,
license, and integration path; and the bake/`verify` evidence pattern (ADR-007) gives a
reproducible way to check that a generated artifact (a font atlas, a shader binary, an ML model
package) matches what was reviewed. None of this *is* §4.1 outsourced-process control — the
manufacturer still has to write the procedure and exercise the control — but each piece is a
concrete input the manufacturer's procedure can cite instead of starting from nothing.

### §4.2 Documentation requirements

#### §4.2.1 General

The QMS documentation must include a quality policy and objectives, a quality manual, documented
procedures and records the standard requires, other documents the organization determines are
necessary for effective process operation, and (per applicable regulatory requirements) the
documentation specified by the regulatory authority. This is the top-level "what exists" list that
§4.2.2 through §4.2.5 elaborate.

MduX-rust does not produce any of this documentation on the manufacturer's behalf — a repository of
governed crates and ADRs is not a quality manual. What it does provide is a documented,
version-controlled design-rationale trail (the [18 accepted ADRs](../adr/README.md)) that a
manufacturer's own procedures can reference as evidence of how a particular design decision was
made and reviewed, in the same spirit that `docs/regulatory-compliance.md` describes the ADR trail
feeding a technical file's design-and-development section.

#### §4.2.2 Quality manual

The quality manual describes (or references procedures describing) the scope of the QMS including
any exclusions and their justification, the documented procedures or references to them, and a
description of the interaction between QMS processes. This is squarely a manufacturer-authored
document — MduX-rust has no analogue to a quality manual, and nothing in this corpus should be read
as implying otherwise. `software_development_file/templates/ISO_13485/` is reserved for a future
manufacturer-facing template skeleton (see `docs/regulatory-compliance.md`'s roadmap section); it is
currently empty, and this corpus does not claim it is populated.

#### §4.2.3 Medical device file

For each medical device type or family, the organization must establish and maintain one or more
files containing (or referencing the location of) records demonstrating conformity to this standard
and applicable regulatory requirements: device specifications, manufacturing/packaging/storage/
handling/distribution specifications, measurement and monitoring procedures, and installation/
servicing requirements where applicable.

For a device that incorporates MduX-rust, the software-specific slice of this file overlaps heavily
with an IEC 62304 software development file's content — device specifications trace to
`mdux_governance::Requirement`s, and the compiled evidence (`ComplianceProgram::trace_matrix_export()`,
generated `package.json`/`report.json` pairs from `tools/mdux-font-baker`/`mdux-shader-baker`/
`mdux-ml-baker`) is exactly the kind of "records demonstrating conformity" this sub-clause asks a
manufacturer to hold. It is one input among many the manufacturer's medical device file needs, not
the file itself — production, packaging, distribution, and servicing records have no MduX-rust
analogue at all, since this project never reaches those life-cycle stages.

#### §4.2.4 Control of documents

Documents required by the QMS must be controlled: reviewed and approved before issue, kept legible
and identifiable, available where needed, protected from unintended use of obsolete versions, and
changes re-reviewed/re-approved. External documents (including standards) determined necessary for
QMS operation must be identified and their distribution controlled.

MduX-rust's own analogue, scoped to source and generated artifacts rather than QMS documents proper,
is `Cargo.lock` committed and built with `--locked` (per `CLAUDE.md`'s replay-CI instructions) for
"what exact version was used," and the bake/`verify` pattern (ADR-007) for "was the currently
committed generated artifact produced from the currently reviewed source" — a CI check that fails
if a generated `package.json` no longer matches its `report.json` digest is a mechanized instance of
"protected from unintended use of an obsolete version," even though it operates on code/asset
artifacts rather than the QMS document set §4.2.4 actually governs.

#### §4.2.5 Control of records

Records required by the QMS must be controlled to provide evidence of conformity and effective QMS
operation: kept legible, readily identifiable and retrievable, protected, and retained for at least
the lifetime of the device as defined by the organization (or per applicable regulatory
requirements), and not less than two years from device release by the organization.

`mdux_governance::AuditEvent` (`crates/mdux-governance/src/lib.rs`) is the record-control mechanism
closest in spirit within MduX-rust's own scope: every `add_requirement`/`add_hazard`/
`add_verification`/`add_problem_report` call is recorded with a sequence number and an
`AuditCategory`, exported via `ComplianceProgram::audit_export()` as an ordered, machine-readable
trail. This is a record of *governance-data changes within a running compliance program instance* —
it has no persistence layer of its own (nothing in `mdux-governance` writes to disk or a database),
so a manufacturer wanting an actual retained record under §4.2.5 must capture and store
`audit_export()`'s output themselves as part of their release process, alongside the retention
period their own regulatory context requires.

---

## Related documents

- [Management and resources](02-management-and-resources.md)
- [Product realisation](03-product-realisation.md)
- [Measurement, analysis and improvement](04-measurement-analysis-improvement.md)
- [AI-Reference index](AI-Reference.md)
- [Citation convention](../governance/citation-convention.md)
