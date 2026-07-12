# IEC 62304: Software problem resolution process

## Module overview

§9 requires a single, documented process for receiving, evaluating, and resolving software
problems — including SOUP anomalies discovered after release — with each report traceable through
to its resolution and any consequent re-verification.

**Key areas covered:**
- Establishing a problem resolution process
- Analyzing and evaluating each problem report
- Traceability from a problem report to its resolution

---

## §9.1 Establish problem resolution process

### §9.1.1-§9.1.2 Receive and document problem reports

`trustsc_governance::ProblemReport { id, summary, closed }` gives every problem report a stable
identity and an explicit open/closed state; `ComplianceProgram::add_problem_report()` records the
report's arrival as a `Lifecycle` `AuditEvent`, so "when was this reported" is derivable from the
sequenced audit trail rather than relying on an external issue tracker's own timestamp as the sole
record.

### §9.1.3 Investigate and analyze

Analysis of a problem report may conclude it represents a previously-unidentified hazard, in which
case it re-enters the risk management process (module 06) rather than being resolved as a pure
implementation defect. This is the same "modification re-enters risk management" principle module 05
(§6.2.2) states for planned changes, applied here to unplanned ones.

### §9.1.4 Approval before release

An open `ProblemReport` (`closed: false`) is exactly the kind of item `§5.8.2`/module 04 requires a
manufacturer to document as a known residual anomaly if a release proceeds with it still open —
`ComplianceProgram::release_evidence_summary()`'s `problems=` count is a quick way to see whether any
exist at release time.

### §9.1.5 Trend analysis

Not currently automated by `trustsc-governance` — the type gives a manufacturer a queryable list of
problem reports (`ComplianceProgram` does not yet expose a public `problems()` accessor alongside
`requirements()`/`audit_events()`), but does not itself perform trend analysis across problem
reports. This is a gap for a manufacturer's own post-market surveillance process to close, not
something this project claims to automate.

## §9.2 Software problem resolution process (evaluation and traceability)

### §9.2.1-§9.2.6 Evaluate, resolve, and verify each problem

Resolution of a problem report follows the same development-process modules (02-04) any other
change does, with the additional requirement that the report itself, and any hazard or requirement
it touches, stay linked. `Hazard.controlled_by` and `VerificationCase.requirement` are the two link
fields that keep this traceable in `trustsc-governance`'s model — a resolution that changes a
requirement's implementation should, if that requirement controls a hazard or has active
verification cases, be re-verified against them rather than assumed correct.

### §9.2.7 Analyze problem reports for trends across SOUP

Where a problem originates in a SOUP component, `docs/governance/soup-register.toml`'s
`component_id` is the join key back to that dependency's registered entry — a manufacturer
resolving several problems traced to the same SOUP component has, in effect, discovered a trend
worth reviewing that entry's `risk_controls` and `support_model` fields for.

---

## Related documents

- [Maintenance process](05-maintenance-process.md)
- [Risk management process](06-risk-management-process.md)
- [Configuration management process](07-configuration-management-process.md)
- [AI-Reference index](AI-Reference.md)
- [Citation convention](../governance/citation-convention.md)
