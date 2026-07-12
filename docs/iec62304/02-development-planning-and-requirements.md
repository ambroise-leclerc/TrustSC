# IEC 62304: Development planning and requirements analysis

## Module overview

The first two sub-clauses of §5 (Software development process): planning the process itself, and
analyzing what the software must do before any design work starts. Both sub-clauses produce
documents that must stay current as the project evolves, not one-time artifacts.

**Key areas covered:**
- The software development plan and its required content
- Selecting and documenting a lifecycle model
- Defining, documenting, and verifying software requirements
- Approval of software requirements

---

## §5.1 Software development planning

### §5.1.1 General

A manufacturer plans the software development process for each software system before starting
detailed design. The plan need not be a single document — for a project like TrustSC it can be
composed of the ADR trail (`docs/adr/`), the CI workflow (`.github/workflows/ci.yml`), and
`docs/architecture.md`, provided together they cover the plan's required content (§5.1.2).

### §5.1.2 Software development plan content

The plan must address, at minimum: the processes/activities/deliverables of development; traceability
between requirements, design, implementation, and verification; the software configuration/problem
resolution processes to use; and the measures used to control coding standards, tools, and
integration environments. TrustSC's equivalent artifacts:

- **Traceability** — `trustsc-governance::ComplianceProgram::trace_rows()`/`trace_matrix_export()`
  (`crates/trustsc-governance/src/lib.rs`) generate a requirement → verification → hazard matrix
  directly from the types a manufacturer populates, rather than a hand-maintained spreadsheet.
- **Configuration management** — module 07 of this corpus; `Cargo.lock` committed and CI building
  `--locked` is the mechanism (ADR-005).
- **Problem resolution** — module 08; GitHub Issues plus `trustsc-governance::ProblemReport`.
- **Coding standards/tools** — `#![forbid(unsafe_code)]` in every governed crate, enforced at
  compile time rather than by review checklist (ADR-005).

### §5.1.3 Keeping the plan current

Every ADR is `Status: Accepted` and dated by its number in `docs/adr/README.md`'s sequence — a
superseding decision gets a new ADR rather than a silent edit, which is itself the update-tracking
mechanism §5.1.3 asks for.

## §5.2 Software requirements analysis

### §5.2.1 Defining requirements

Software requirements state what the software must do, not how — functional/capability behavior,
performance, interfaces with hardware/other software, and (per §5.2.2, folded in below) required
risk control measures. `trustsc_governance::Requirement { id, title, source_clause, verification_intent }`
gives every requirement a stable id, a citable clause it traces back to, and a stated verification
intent up front, rather than letting verification strategy be decided after the fact.

### §5.2.2 Include risk control measures in requirements

Where risk analysis (module 06) identifies a software risk control measure, that measure must
itself become a software requirement, not remain a standalone mitigation note. `Hazard::controlled_by`
(a list of `RequirementId`s, required to be non-empty) is exactly this link: a hazard's mitigation
must exist as an actual `Requirement`, checkable by `ComplianceProgram::validate()`.

### §5.2.3 Requirements re-evaluation and update

Requirements evolve as design and testing surface new information; each change goes through the
same approval described below rather than being edited silently. `trustsc_governance::AuditEvent`
(category `Lifecycle`) records `add_requirement` calls with a sequence number, giving requirement
changes a timestamped-by-sequence trail (`ComplianceProgram::add_requirement`).

### §5.2.4 Verify software requirements

Requirements should be unambiguous, testable, and traceable before design starts. Every
`Requirement` must have at least one `VerificationCase` referencing it or `ComplianceProgram::validate()`
fails — see `docs/iec62304/04-development-implementation-and-testing.md` for the verification-case
side of this pairing.

### §5.2.5 Requirements approval

TrustSC does not prescribe a specific approval workflow (that's the manufacturer's QMS role per
§4.1). Its `.medui` DSL has a `@safety_critical` annotation and a per-node `requirement` identifier
that ADR-011 intends to be bound together, so an approved requirement stays traceable to the UI
element it governs — but as of this writing the `.medui` compiler
(`crates/trustsc-ui-dsl-authoring`) does not enforce that pairing at build time: the two are independent
attributes, and a `@safety_critical` node with no `requirement` currently compiles without error.
This is a gap between ADR-011's stated intent and the current implementation, not a build-time
guarantee to rely on today.

---

## Related documents

- [Scope and general requirements](01-scope-and-general-requirements.md)
- [Development design](03-development-design.md)
- [Implementation and testing](04-development-implementation-and-testing.md)
- [Risk management process](06-risk-management-process.md)
- [AI-Reference index](AI-Reference.md)
- [Citation convention](../governance/citation-convention.md)
