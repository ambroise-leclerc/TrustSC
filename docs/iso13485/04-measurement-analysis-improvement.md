# ISO 13485: Measurement, analysis and improvement

## Module overview

§8 closes the QMS loop: having planned (§4-§5), resourced (§6), and executed product realization
(§7), the organization must monitor and measure whether the QMS and the product actually work,
contain nonconformity when it occurs, analyze data for trends, and act on what it learns. This is
the clause with the strongest surface-level resemblance to IEC 62304 §9 (problem resolution, see
`docs/iec62304/08-problem-resolution-process.md`) and to this project's own honestly-stated gaps —
several sub-clauses below are places TrustSC explicitly does not automate a post-market or
QMS-level activity, and this module says so rather than stretching a software type to cover a role
it doesn't fill.

**Key areas covered:**
- General measurement, analysis, and improvement obligations
- Monitoring and measurement: feedback, complaint handling, regulatory reporting, internal audit,
  process and product measurement
- Control of nonconforming product
- Analysis of data
- Improvement: corrective action and preventive action

---

## §8.1 General

The organization must plan and implement the monitoring, measurement, analysis, and improvement
processes needed to demonstrate conformity of product, ensure QMS conformity, and maintain QMS
effectiveness — determining applicable methods, including statistical techniques, and the extent of
their use.

`trustsc_governance::ComplianceProgram` is a measurement instrument for one specific, narrow slice of
this general obligation: the software requirement/verification/hazard coverage of a device's
governed UI and ML layers. `ComplianceProgram::validate()` and `release_evidence_summary()` give a
manufacturer a repeatable, automatic method for checking one part of product conformity (every
requirement verified, every Class C device hazard-analyzed) — a concrete instance of "determine
applicable methods" for that slice, not a general answer to §8.1's much broader scope, which spans
process performance, customer satisfaction, audit findings, and product characteristics across the
whole QMS.

## §8.2 Monitoring and measurement

### §8.2.1 Feedback

The organization must gather and monitor information on whether it has met customer requirements, as
an early-warning input to detecting quality problems, with the method for gathering and using this
feedback documented.

No TrustSC analogue: this project has no end customers of a finished medical device and gathers no
post-market feedback. A manufacturer's feedback process about their device is entirely their own, and
nothing in `trustsc-governance` represents customer feedback as a concept.

### §8.2.2 Complaint handling

The organization must document procedures for timely complaint handling per applicable regulatory
requirements, including at minimum: receipt/recording, evaluation of whether the complaint
represents a reportable event, investigation (with rationale if not investigated), determination of
regulatory reporting need, handling of complaints related to counterfeit product, and record
retention.

`trustsc_governance::ProblemReport { id, summary, closed }` is deliberately *not* built to be this — it
is the software project's own defect record (closer to IEC 62304 §9's problem report, see
`docs/iec62304/08-problem-resolution-process.md`), tracking whether an issue found in TrustSC's own
development or a manufacturer's integration testing is open or resolved. It has no field for
"received from a customer," no reportable-event evaluation, and no regulatory-reporting workflow. A
manufacturer's actual post-market complaint handling process is a distinct QMS procedure this
project does not provide, and `ProblemReport` should not be repurposed as if it were one.

### §8.2.3 Reporting to regulatory authorities

Where applicable regulatory requirements specify notification of adverse events or issuance of
advisory notices meeting reportability criteria, the organization must have documented procedures
for such reporting.

No TrustSC analogue whatsoever — this is a manufacturer-to-regulator communication obligation that
presupposes a marketed device and a post-market surveillance capability neither this project nor its
governance types attempt to model.

### §8.2.4 Internal audit

The organization must conduct internal audits at planned intervals to determine whether the QMS
conforms to planned arrangements, applicable requirements, and this standard, and is effectively
implemented and maintained — with an audit programme accounting for the status/importance of the
processes and areas audited and results of previous audits, documented audit criteria/scope/
frequency/methods, auditor objectivity/impartiality, and defined responsibilities for reporting
results and taking timely correction/corrective action, with follow-up verifying actions taken and
their effectiveness.

`ComplianceProgram::trace_matrix_export()` and `audit_export()` produce artifacts an internal auditor
reviewing the software-development slice of the QMS could use as audit evidence — a current,
machine-derived trace matrix is easier to audit against than a hand-maintained spreadsheet that may
already have drifted from the code. Neither export constitutes an internal audit: there is no
audit programme, no independence/objectivity check on who generates the export, and no
corrective-action follow-up loop. `AuditEvent`'s name is a naming coincidence worth flagging
explicitly — it records governance-data lifecycle events (a requirement or hazard being added), not
QMS internal-audit events in this sub-clause's sense; a reader should not infer §8.2.4 coverage from
the type name alone.

### §8.2.5 Monitoring and measurement of processes

The organization must apply suitable methods for monitoring and, as applicable, measuring QMS
processes, methods demonstrating the processes' ability to achieve planned results, with
corrective action taken when planned results are not achieved and process effectiveness not
maintained.

For the two processes TrustSC most directly instruments — asset-pipeline baking and UI/ML
verification — `.github/workflows/ci.yml` running every baker's `verify` subcommand and
`--verify-ui`'s checks on every push is continuous process monitoring in the sense this sub-clause
asks for: a build that fails to reproduce its committed evidence, or a screen that fails to render
within its `GoldenBounds`, is the process failing to achieve its planned result, surfaced
immediately rather than at the next scheduled audit. This is real, but scoped only to the two
processes named — the QMS's other processes (purchasing, design review scheduling, training) have no
equivalent automated monitoring within TrustSC's scope.

### §8.2.6 Monitoring and measurement of product

The organization must monitor and measure product characteristics to verify requirements are met,
at appropriate stages of product realization per the planned arrangements, maintaining evidence of
conformity with the acceptance criteria including who authorized product release, and not proceeding
with product release/service delivery until planned arrangements have been satisfactorily completed
(unless approved by a relevant authority and, where applicable, the customer).

`ComplianceProgram::validate()` is a release gate matching this sub-clause's "do not proceed until
planned arrangements are satisfactorily completed" principle for the software governance data
specifically: it fails if any requirement lacks a verification case, if any verification case
references a nonexistent requirement (orphan rejection), or if a Class C device has zero recorded
hazards. `--verify-ui`'s rendered-truth checks and each baker's `verify` subcommand are the
"maintain evidence of conformity with acceptance criteria" half of this sub-clause, applied to UI
rendering and generated assets respectively. None of these constitute product monitoring and
measurement for the device as a whole — physical characteristics, clinical performance, and
non-software requirements are entirely outside what any of these mechanisms check.

## §8.3 Control of nonconforming product

### §8.3.1-§8.3.4 General, pre-delivery actions, post-delivery actions, and rework

Product not conforming to requirements must be identified and controlled to prevent unintended use
or delivery, with documented procedures defining controls, responsibilities, and authorities for
identification, documentation, segregation, evaluation, and disposition — including, after delivery,
actions appropriate to the effects (or potential effects) of the nonconformity, up to and including
advisory notices or product recall. Rework must be documented, including its effect on product
conformity, and re-verified per documented procedures once complete.

No dedicated TrustSC type models "nonconforming product" as this sub-clause defines it — that
concept applies to a manufactured device, not a software library. The nearest adjacent mechanism is
negative: `ComplianceProgram::validate()` refusing to succeed (an orphaned verification case, a
Class C device with no hazard, a requirement with zero verification cases) is closer to *preventing
an invalid governance-data set from being treated as release-ready* than to §8.3's product-level
nonconformity control, and a `ProblemReport` with `closed: false` is a known, open issue rather than
a controlled/segregated nonconforming unit. A manufacturer's actual nonconforming-product process,
covering physical units and shipped software builds alike, is a QMS procedure this project does not
provide.

## §8.4 Analysis of data

The organization must determine, collect, and analyze appropriate data to demonstrate QMS
suitability, adequacy, and effectiveness, including data generated by monitoring/measurement and
other relevant sources, covering at minimum feedback, product-requirement conformity, process/
product characteristics and trends (including opportunities for improvement), supplier performance,
audit results, and service reports where applicable — with analysis methods including appropriate
statistical techniques and results recorded.

`trustsc_governance` exposes the raw material for one slice of this analysis (requirements, hazards,
verification cases, problem reports, and the sequenced audit trail) but performs no aggregation,
trend detection, or statistical analysis over it itself — `docs/iec62304/08-problem-resolution-process.md#915-trend-analysis`
notes the identical gap from the IEC 62304 angle: `ComplianceProgram` has no public accessor
aggregating problem reports for trend purposes today, and nothing computes rates, distributions, or
correlations across the data it holds. This is a deliberate scope boundary, not an oversight to be
quietly worked around — a manufacturer's own data-analysis process, potentially consuming
`audit_export()`'s output as one raw input among many, is where §8.4's actual analysis happens.

## §8.5 Improvement

### §8.5.1 General

The organization must identify and implement changes needed to ensure and maintain product
conformity, QMS suitability and effectiveness, and safety/performance of the device throughout its
lifetime, using the quality policy, quality objectives, audit results, data analysis, corrective and
preventive actions, and management review as inputs.

### §8.5.2 Corrective action

The organization must take action to eliminate the cause of nonconformities to prevent recurrence,
appropriate to the effects of the nonconformities encountered, via a documented procedure defining
requirements for reviewing nonconformities (including complaints), determining causes, evaluating
the need for action to ensure nonconformities do not recur, planning and documenting/implementing
needed action including updating documentation as appropriate, verifying the action does not
adversely affect the ability to meet applicable regulatory requirements or safety/performance, and
reviewing the effectiveness of corrective action taken.

### §8.5.3 Preventive action

The organization must determine action to eliminate the causes of *potential* nonconformities to
prevent their occurrence, appropriate to the effects of the potential problems, via a documented
procedure with requirements for determining potential nonconformities and their causes, evaluating
the need for preventive action, planning/documenting/implementing needed action, verifying it does
not adversely affect the ability to meet applicable regulatory requirements or safety/performance,
and reviewing the effectiveness of preventive action taken.

Together these three sub-clauses describe a closed-loop CAPA (corrective and preventive action)
process that TrustSC does not implement. The connective tissue it does provide is traceability
once a manufacturer's own CAPA process identifies a needed change: a `ProblemReport` transitioning
from open to closed, and any requirement or hazard that change touches, stay linked via
`Hazard.controlled_by` and `VerificationCase.requirement` — the same linkage
`docs/iec62304/08-problem-resolution-process.md#921-926-evaluate-resolve-and-verify-each-problem`
describes for IEC 62304 §9.2's evaluation-and-traceability requirement, which is the same underlying
mechanism serving both standards' resolution-traceability expectations. What is genuinely missing,
and stated as a gap rather than implied to be covered: root-cause determination, action planning and
approval, effectiveness review, and the "does this action adversely affect safety/regulatory
compliance" check are all judgment calls a manufacturer's own CAPA procedure makes — no type in
`trustsc-governance` performs or records any of them today.

---

## Related documents

- [Foundations and quality management system](01-foundations-and-qms.md)
- [Management and resources](02-management-and-resources.md)
- [Product realisation](03-product-realisation.md)
- [IEC 62304 problem resolution process](../iec62304/08-problem-resolution-process.md)
- [AI-Reference index](AI-Reference.md)
- [Citation convention](../governance/citation-convention.md)
