# IEC 62304: Software risk management process

## Module overview

§7 is the point where IEC 62304 and ISO 14971 meet: software-specific hazard analysis, the risk
control measures that follow from it, verifying those measures actually work, and re-running this
analysis whenever software changes. See `docs/iso14971/` for the general risk-management process
this clause specializes for software.

**Key areas covered:**
- Identifying how software could contribute to a hazardous situation
- Risk control measures and their implementation as requirements
- Verifying risk control measures
- Risk management of software changes

---

## §7.1 Analysis of software contributing to hazardous situations

### §7.1.1-§7.1.3 Identify and evaluate contributing sequences

For each identified hazard, the manufacturer analyzes whether and how software could contribute to
the sequence of events leading to it, and estimates the resulting risk. `trustsc_governance::Hazard`
(`id`, `description`, `controlled_by`) is a structured record of the outcome of this analysis — not
the analysis process itself, which remains the manufacturer's engineering judgment, but a place to
record which requirements exist *because of* that analysis.

`ComplianceProgram::validate()` requires at least one `Hazard` when `SafetyClass::C` — a Class C
device's risk analysis output must be recorded, or the compliance program is incomplete by this
project's model. Class B carries no such minimum, since not every Class B software item necessarily
contributes to a hazardous situation.

## §7.2 Risk control measures

### §7.2.1 Identify software risk control measures needed

Where analysis (§7.1) identifies that software contributes to a hazard, a control measure is
selected and — per §5.2.2 (module 02) — expressed as a software requirement. `Hazard.controlled_by`
is the pointer from a hazard to the requirement(s) implementing its control(s); `Hazard::validate()`
rejects a hazard with no controlling requirement, so this link cannot be left implicit.

A concrete instance in the example applications: `examples/class_c_monitor`'s sedation-index alert
path exists because a delayed or missed alert is a hazardous-contribution scenario for a
depth-of-anesthesia monitor — `Classifier1D::predict()`'s deterministic, allocation-free inference
and `trustsc-ml-runtime`'s fail-closed self-test at startup (ADR-017) are risk control measures for
that hazard, not incidental performance properties.

### §7.2.2 Implement risk control measures

Implemented per module 03/04's design and unit-verification process — a risk control measure gets
no separate implementation track from any other requirement.

## §7.3 Verification of risk control measures

### §7.3.1 Verify implementation

Each requirement that exists as a risk control measure needs a `VerificationCase` like any other
requirement (module 02, module 04) — `ComplianceProgram::validate()`'s "every requirement has ≥1
verification" rule applies uniformly, so a risk control measure cannot be recorded without also
being verified.

### §7.3.2 Verify no new hazards introduced

Introducing a risk control measure must not itself create a new hazardous contribution — e.g. adding
`trustsc-ml-runtime`'s startup self-test could in principle introduce a new failure mode (a false
self-test failure blocking a device that would otherwise function correctly); ADR-017's design
explicitly accepts "fail closed" as the safer of the two failure directions for this tradeoff.

## §7.4 Risk management of software changes

A change to software already in the field re-enters this process before release — see module 05
(§6.2.2) for how modification analysis and risk management connect. `trustsc_governance::AuditEvent`'s
`Verification` category (recorded by `ComplianceProgram::add_verification`) gives a change's
re-verification a place in the sequenced audit trail.

---

## Related documents

- [Development planning and requirements](02-development-planning-and-requirements.md)
- [Development design](03-development-design.md)
- [Maintenance process](05-maintenance-process.md)
- [ISO 14971 risk management corpus](../iso14971/README.md)
- [AI-Reference index](AI-Reference.md)
- [Citation convention](../governance/citation-convention.md)
