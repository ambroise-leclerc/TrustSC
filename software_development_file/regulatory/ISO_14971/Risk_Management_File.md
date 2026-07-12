# Risk Management File — MduX-rust

> Filled-in example for MduX-rust itself. See
> [`software_development_file/templates/ISO_14971/Risk_Management_File.md`](../../templates/ISO_14971/Risk_Management_File.md)
> for the blank template, [`docs/iso14971/README.md`](../../../docs/iso14971/README.md) for the
> underlying clause-by-clause guidance, and
> [`docs/iec62304/06-risk-management-process.md`](../../../docs/iec62304/06-risk-management-process.md)
> for the software-specific slice of this process.

## Document control

- **Product / software item:** MduX-rust
- **Scope note:** this file documents how MduX-rust's `mdux-governance` types and design mechanisms
  support a manufacturer's own ISO 14971 risk management file for a device built on it — it is not a
  risk management file for a specific finished device, since MduX-rust has no clinical intended use
  of its own.

## 1. Risk management plan summary

> `ISO 14971:2019 §4 General requirements for risk management system`

`mdux_core::DeviceContext.safety_class` (`Class B`/`Class C`) is the top-level classification driving
how much of `mdux-governance`'s enforcement applies: a `Class C` device's `ComplianceProgram` must
record at least one `Hazard`, per `ComplianceProgram::validate()`.

## 2. Risk analysis

> `ISO 14971:2019 §5 Risk analysis`

`mdux_governance::Hazard { id, description, controlled_by }` records the outcome of a manufacturer's
hazard analysis — `Hazard::validate()` rejects a hazard with an empty `controlled_by` list, so a
hazard cannot be recorded without at least one requirement addressing it. `examples/class_c_monitor`
(NeuroSense 500) is a worked example: a delayed or missed sedation-index alert is treated as a
hazardous-contribution scenario, controlled by requirements around `Classifier1D::predict()`'s
deterministic, allocation-free inference latency and the fail-closed self-test at startup
([ADR-017](../../../docs/adr/ADR-017-zero-soup-ml-inference-pipeline.md)).

## 3. Risk evaluation

> `ISO 14971:2019 §6 Risk evaluation`

Not automated by `mdux-governance` — deciding whether an estimated risk is acceptable as-is is the
manufacturer's clinical/regulatory judgment. `mdux-governance` records the *outcome* of that judgment
(which hazards have controls) but does not perform the evaluation itself.

## 4. Risk control

> `ISO 14971:2019 §7 Risk control`

Every risk control measure is required to exist as an actual `Requirement`
(`crates/mdux-governance/src/lib.rs`), not a free-floating note — `Hazard.controlled_by` is a list of
`RequirementId`s. Each such `Requirement` must in turn have at least one `VerificationCase`
(`ComplianceProgram::validate()`), so a risk control measure cannot be recorded as implemented
without also being verified.

## 5. Overall residual risk evaluation

> `ISO 14971:2019 §8 Evaluation of overall residual risk`

Not automated — `ComplianceProgram::release_evidence_summary()` gives a release-time snapshot
(`hazards=`, `verifications=` counts) useful as an input to this judgment, but the judgment itself —
whether overall residual risk is acceptable — remains the manufacturer's.

## 6. Risk management review

> `ISO 14971:2019 §9 Risk management review`

The `mdux_governance::AuditEvent` trail (`Lifecycle`/`Verification` categories, sequenced) gives a
reviewer a chronological record of when each hazard, requirement, and verification was added,
usable as supporting evidence that the plan in §1 was actually executed in the order claimed.

## 7. Production and post-production activities

> `ISO 14971:2019 §10 Production and post-production activities`

`mdux_governance::ProblemReport { id, summary, closed }` is where field information (via a
manufacturer's own complaint/incident intake) would be recorded once triaged into MduX-rust's
governance model — this project provides the record type, not the intake process itself; see
[`docs/iec62304/08-problem-resolution-process.md`](../../../docs/iec62304/08-problem-resolution-process.md).

## Justification records

```json
{
  "justification_id": "JUS-007",
  "standard": "ISO 14971",
  "clause_ref": "ISO 14971:2019 §7.1 Risk reduction",
  "rationale": "Hazard.controlled_by requires at least one Requirement, and that Requirement in turn requires at least one VerificationCase (ComplianceProgram::validate()), so a risk control measure cannot be recorded as in place without also being tied to verification evidence.",
  "evidence_refs": [
    "crates/mdux-governance/src/lib.rs",
    "docs/adr/ADR-017-zero-soup-ml-inference-pipeline.md",
    "examples/class_c_monitor"
  ]
}
```
