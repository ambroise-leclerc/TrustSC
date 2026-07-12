# ISO 14971: Production and post-production activities

## Module overview

§10 closes the loop the earlier modules open: once a device is in production and on the market, a
manufacturer must keep collecting and reviewing information about it, feed anything relevant back
into the risk management file (module 01 §4.5), and act when that information reveals a new hazard,
an inadequately controlled risk, or an opportunity to reduce risk further. This is a genuinely
post-market clause — MduX-rust, as a library rather than a fielded device, has no production or
post-production activity of its own; every mechanism cited below is scoped to *what a manufacturer's
own post-market process could draw on from a device built on MduX-rust*, not to anything MduX-rust
performs itself.

**Key areas covered:**
- General production and post-production monitoring obligations
- Collection of information from production and post-production sources
- Review of experience against the risk management file
- Actions triggered when new information changes the risk picture

---

## §10.1 General

A manufacturer establishes a system to collect and review information about their device throughout
production and post-production, feeding it back into the risk management process for the device's
entire lifecycle — not only at initial release. MduX-rust has no post-market presence: it is
consumed at build time, and nothing in the governed crates persists or transmits information about a
fielded device back to this project. Everything a manufacturer's §10 system collects comes from their
own field data, complaint handling (ISO 13485 §8.2.2 — see
[`docs/iso13485/04-measurement-analysis-improvement.md §8.2.2`](../iso13485/04-measurement-analysis-improvement.md#822-complaint-handling)),
and post-market surveillance activities, none of which MduX-rust performs on their behalf.

## §10.2 Collection of information

Sources of production and post-production information include manufacturing/production data, user
and servicing feedback, publicly available information about similar devices, and applicable
regulatory or standards developments. `mdux_governance::ProblemReport { id, summary, closed }`
(`crates/mdux-governance/src/lib.rs`) is the nearest MduX-rust type to an information-collection
record, but — as module 01 and the IEC 62304 corpus both note —
([`docs/iec62304/08-problem-resolution-process.md`](../iec62304/08-problem-resolution-process.md))
it is scoped to defects found in MduX-rust's own development or a manufacturer's integration testing,
not to field reports from a marketed device's actual users. A manufacturer's own post-market
information-collection system is a separate process this project supplies no scaffolding for.

## §10.3 Review of experience

Collected information is reviewed to determine whether it reveals a previously unidentified hazard or
hazardous situation, whether an estimated risk (or a risk arising from a control measure, per module
03 §7.6) is no longer acceptably controlled, or whether the state of the art has changed such that
further risk reduction is now practicable. Where such a review concerns the software slice of a
device built on MduX-rust, `Hazard`/`Requirement`/`VerificationCase`'s existing linkage —
`Hazard.controlled_by`, `VerificationCase.requirement` — gives a reviewer a traceable path from a
newly-relevant piece of field information back to the specific requirement(s) and verification
evidence that would need to be revisited, the same linkage
[`docs/iec62304/08-problem-resolution-process.md §9.2`](../iec62304/08-problem-resolution-process.md#92-software-problem-resolution-process-evaluation-and-traceability)
describes for IEC 62304's problem-resolution traceability. The review judgment itself — does this
field observation actually indicate a new or under-controlled risk — remains entirely the
manufacturer's.

## §10.4 Actions

Where review of experience indicates action is needed, the manufacturer determines and implements it
through the same risk management process modules 02-03 describe (a re-run of risk analysis, risk
evaluation, and risk control for the newly-relevant hazard or hazardous situation), rather than a
separate post-market-only track — mirroring
[`docs/iec62304/05-maintenance-process.md §6.2.2`](../iec62304/05-maintenance-process.md#622-use-software-risk-management-process-for-modifications)'s
"a modification re-enters risk management" principle, applied here to modifications triggered by
field experience rather than a planned change. Where the action is a software change, it flows
through `mdux-governance`'s ordinary requirement/verification machinery exactly as any other risk
control measure does (module 03 §7.3) — recorded, controlled by a hazard, and independently verified
— and through IEC 62304's problem resolution process
([`docs/iec62304/08-problem-resolution-process.md`](../iec62304/08-problem-resolution-process.md))
if it originated from a problem report.

---

## Related documents

- [Scope and general requirements](01-scope-and-general-requirements.md)
- [Risk analysis and evaluation](02-risk-analysis-and-evaluation.md)
- [Risk control and residual risk](03-risk-control-and-residual-risk.md)
- [IEC 62304 problem resolution process](../iec62304/08-problem-resolution-process.md)
- [AI-Reference index](AI-Reference.md)
- [Citation convention](../governance/citation-convention.md)
