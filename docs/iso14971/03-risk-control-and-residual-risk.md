# ISO 14971: Risk control and residual risk

## Module overview

This module covers §7 (Risk control, in full: §7.1-§7.7), §8 (Evaluation of overall residual risk),
and §9 (Risk management review) — the back half of the risk management process: what happens once
risk evaluation (module 02 §6) finds a risk that is not already acceptable, how the resulting control
measures are implemented and verified, how the residual risk left over is judged, and the review gate
that confirms the whole process was actually carried out before a device ships. `docs/iec62304/06-risk-management-process.md`
already documents the software-specific slice of implementing and verifying software risk control
measures; this module cites it rather than duplicating its content.

**Key areas covered:**
- Risk reduction and the analysis of risk control options
- Implementing and verifying risk control measures
- Residual risk evaluation and risk-benefit analysis
- Risks introduced by risk control measures themselves
- Completeness of risk control across all identified hazards
- Evaluating overall residual risk and the pre-release risk management review

---

## §7.1 Risk reduction

Where risk evaluation finds a risk that is not acceptable, one or more risk control measures must be
selected and implemented to reduce it. Standard risk-management practice favors measures that change
the design itself over measures that compensate for a design left unchanged, and favors both of those
over measures that merely inform a user of a residual danger — a preference order the manufacturer
applies when choosing among available options, not something a UI/ML SDK can decide on the
manufacturer's behalf.

What TrustSC does provide is the structural guarantee that a control measure is actually expressed
as engineering work rather than left as a paper intention: `Hazard.controlled_by`
(`crates/trustsc-governance/src/lib.rs`), a non-empty list of `RequirementId`s, is `Hazard::validate()`'s
enforced link from a hazard to at least one requirement that must exist and be verified (module 01
§4.4 in the IEC 62304 corpus covers the requirement side of this link in detail). Whether the chosen
requirement implements an inherently safe design change, a protective measure, or an information
disclosure is a property of what the requirement says, not something the type system distinguishes.

## §7.2 Risk control option analysis

Before implementing a chosen control, a manufacturer analyzes the available options for effectiveness
and for any new risk they might introduce (a concern §7.6 revisits directly once a measure is
implemented). This analysis is engineering and clinical judgment applied to the specific hazardous
situation and has no TrustSC automation. One structural pattern in this repository is worth noting
as an analog a manufacturer might find useful to imitate, without it being the same activity: an ADR
under `docs/adr/` documents not just the decision made but the options considered and rejected and why
(see, for example, ADR-018's discussion of why `SignalTrace` was added as a dedicated primitive rather
than overloading `VulkanViewport`) — the same discipline of writing down rejected alternatives and
their reasoning is good practice for a device-level risk control option analysis, even though an ADR
records a software architecture decision, not a patient-safety risk control decision.

## §7.3 Implementation of risk control measures

Once a control measure is expressed as a software requirement, its implementation runs through the
ordinary software development process — `docs/iec62304/06-risk-management-process.md` §7.2.2 already
covers this: a risk control measure gets no separate implementation track from any other requirement,
and every requirement (risk-control or otherwise) needs at least one `VerificationCase`
(`ComplianceProgram::validate()`, `crates/trustsc-governance/src/lib.rs`) or the compliance program fails
to validate.

`examples/class_c_monitor` (NeuroSense 500) is a fully worked instance of this clause end to end.
The hazardous situation (module 02 §5.4) is a delayed or missed sedation-index alert. The risk control
measures are: `Classifier1D::predict()`'s deterministic, allocation-free inference path over each
64-bin EEG spectral row (no heap allocation, no unbounded-time operation in the hot path), and
`trustsc-ml-runtime`'s startup self-test, which re-runs every golden input/output vector baked into the
model package and fails closed — refusing to construct — if any output diverges bit-for-bit from what
was recorded (ADR-017). Both are implemented as ordinary Rust code verified by `cargo test` and the CI
`verify` step against `generated/models/eeg-demo/`, exactly like any other requirement.

A second, more modest example from the same device: `examples/class_c_monitor` also renders a raw-EEG
`SignalTrace` (ADR-018) alongside the derived sedation index, mirroring the practice of real
depth-of-anesthesia monitors that always show the raw signal next to a derived number. Presenting the
underlying signal alongside a derived index is a recognized way to reduce a clinician's over-reliance
on a single computed value — a plausible risk-control rationale for the feature, though it is worth
being precise that ADR-018 itself documents `SignalTrace` as a rendering-primitive decision and does
not frame it in ISO 14971 terms; a manufacturer citing this pattern in their own risk management file
would need to write that justification themselves rather than pointing at ADR-018 as if it already
says so.

## §7.4 Residual risk evaluation

After a control measure is implemented, the risk that remains is re-estimated and re-evaluated against
the same acceptability criteria used in module 02 §6. `docs/iso14971/schemas/risk-record.schema.json`'s
`residual_risk_acceptable` boolean and `risk_control_measures` array (ideally populated with the
`RequirementId`s that were credited) are the structured place this outcome is recorded, alongside which
measures were credited for it. As noted in module 02, TrustSC computes no residual risk value itself
— it has no quantitative risk model — so this field always reflects a judgment made outside the
schema, not a computed result.

## §7.5 Risk-benefit analysis

Where a residual risk remains unacceptable by the manufacturer's own criteria, and no further risk
control is practicable, a risk-benefit analysis weighing the residual risk against the medical benefit
of the device is required. This is a clinical and regulatory judgment call that sits well outside what
a UI/ML rendering SDK can inform, let alone automate — TrustSC provides no scaffolding for this
subclause, and none should be inferred from anything in this repository.

## §7.6 Risks arising from risk control measures

Implementing a control measure must not itself introduce a new hazard or increase an existing risk.
`docs/iec62304/06-risk-management-process.md` §7.3.2 already covers a concrete instance of this exact
reasoning: introducing `trustsc-ml-runtime`'s startup self-test could in principle create a new failure
mode (a false self-test failure blocking a device that would otherwise function correctly), and
ADR-017's design explicitly accepts "fail closed" as the safer of the two possible failure directions
for that specific tradeoff — a documented instance of a manufacturer's engineering team performing
exactly the analysis this subclause asks for, rather than an automated check.

## §7.7 Completeness of risk control

Before moving on, a manufacturer confirms that every identified hazard and hazardous situation has
actually been addressed — none silently dropped between analysis and implementation.
`Hazard::validate()`'s non-empty `controlled_by` requirement gives a partial, machine-checked floor for
this: a hazard that has been *recorded* cannot be left without at least one controlling requirement, so
`ComplianceProgram::validate()` will reject an orphaned hazard record. This is deliberately a narrower
guarantee than the subclause as a whole asks for — it cannot detect a hazard that was never recorded in
the first place, only that every recorded hazard has some control. Whether the underlying analysis
(module 02 §5.4) was itself complete remains an engineering judgment no software check can substitute
for.

---

## §8 Evaluation of overall residual risk

Once individual risks have been controlled and their residual risk judged acceptable one at a time,
the manufacturer evaluates the *overall* residual risk presented by the device as a whole — because
several individually acceptable risks can combine into an overall picture that needs its own judgment
against the device's benefits. `ComplianceProgram::trace_rows()`/`release_evidence_summary()`
(`crates/trustsc-governance/src/lib.rs`) give an aggregate, structured export — counts of requirements,
hazards, verifications, and problem reports for a device — that could serve as one input to this
evaluation, but TrustSC performs no aggregation of risk *values* and reaches no acceptability
conclusion of its own. The overall judgment remains the manufacturer's; this repository supplies
inputs a reviewer could consult, not the review's outcome.

## §9 Risk management review

Before a device is released, a review confirms the risk management plan (module 01 §4.4) was actually
carried out as planned, that overall residual risk (§8) is acceptable, and that appropriate methods
are in place to collect and review production and post-production information (module 04). Two
existing mechanisms give this review structured evidence to draw on:
`ComplianceProgram::validate()` is a machine-checked, narrower pre-release gate — every requirement
has at least one verification case, every verification case references a requirement that actually
exists, and a Class C device has at least one recorded hazard — and `AuditEvent`'s `Release` category
(`AuditCategory::Release`) gives such a review's outcome a place in the sequenced audit trail.

Neither constitutes the review itself. The review this subclause describes is a documented sign-off by
an accountable person (module 01 §4.2's management-responsibility obligation), and no type in
`trustsc-governance` today represents that sign-off as a first-class record — `ComplianceProgram::validate()`
succeeding is evidence a reviewer could point to, not a substitute for the review having actually taken
place.

---

## Related documents

- [Scope and general requirements](01-scope-and-general-requirements.md)
- [Risk analysis and evaluation](02-risk-analysis-and-evaluation.md)
- [Production and post-production activities](04-production-and-post-production.md)
- [IEC 62304 software risk management process](../iec62304/06-risk-management-process.md)
- [AI-Reference index](AI-Reference.md)
- [Citation convention](../governance/citation-convention.md)
