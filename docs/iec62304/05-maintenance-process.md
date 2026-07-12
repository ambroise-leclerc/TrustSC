# IEC 62304: Software maintenance process

## Module overview

§6 governs what happens to software after release: how a manufacturer plans ongoing maintenance,
analyzes incoming problems/modification requests against risk, and implements changes through the
same rigor as original development rather than a shortcut path.

**Key areas covered:**
- The software maintenance plan
- Problem and modification analysis, including re-classification triggers
- Modification implementation using the original development process

---

## §6.1 Establish software maintenance plan

A maintenance plan states how problem reports and change requests are received, evaluated, and
released post-market. TrustSC's own maintenance-relevant mechanisms — CI running on every push
(`.github/workflows/ci.yml`), the `Cargo.lock`-pinned, `--locked`-built dependency set (ADR-005),
and the bake/`verify` pattern that re-derives evidence on demand (ADR-007) — are the scaffolding a
manufacturer's maintenance plan can build its own process on top of, not a substitute for one.

## §6.2 Problem and modification analysis

### §6.2.1 Analyze problem reports and modification requests

Every `trustsc_governance::ProblemReport` (`id`, `summary`, `closed`) is a discrete, trackable analysis
unit; `ComplianceProgram::add_problem_report()` records a `Lifecycle` audit event so the analysis
step itself is timestamped-by-sequence in the audit trail (`AuditEvent`), not just its resolution.

### §6.2.2 Use software risk management process for modifications

A modification that could affect classification, a hazard's controls, or a requirement's
verification status must re-enter the risk management process (module 06) rather than being treated
as a pure implementation change. `Hazard.controlled_by` staying non-empty and every `Requirement`
retaining at least one `VerificationCase` (`ComplianceProgram::validate()`) is the machine-checkable
form of "the modification hasn't silently broken an existing control."

## §6.3 Modification implementation

### §6.3.1 Use change control procedures

Modifications flow through the same development process modules 02-04 describe — requirements
analysis, design, implementation, verification — rather than a separate lightweight track. This
project's own convention of stacking related changes as reviewed, sequentially-merged units (see
`docs/adr/README.md`'s ADR sequence, each capturing one accepted decision with its consequences) is
the audit-trail-friendly expression of this: a modification's rationale is recorded alongside the
change, not reconstructed after the fact from a commit message.

---

## Related documents

- [Implementation and testing](04-development-implementation-and-testing.md)
- [Risk management process](06-risk-management-process.md)
- [Configuration management process](07-configuration-management-process.md)
- [Problem resolution process](08-problem-resolution-process.md)
- [AI-Reference index](AI-Reference.md)
- [Citation convention](../governance/citation-convention.md)
