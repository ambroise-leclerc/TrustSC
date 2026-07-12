# ISO 14971: Scope and general requirements for risk management system

## Module overview

This module covers ISO 14971:2019's front matter — scope, normative references, and terms and
definitions — and its cross-cutting §4 clause: the organizational and process obligations a
manufacturer must have in place before any device-specific risk analysis (module 02) can begin.
Everything in modules 02-04 assumes a risk management process, a plan, and a file already exist per
this clause; MduX-rust does not create any of those three for a manufacturer, but several of its
governed types are natural places to record the outputs a manufacturer's process produces.

This whole `docs/iso14971/` folder is the sister document to
[`docs/iec62304/06-risk-management-process.md`](../iec62304/06-risk-management-process.md): that
module already covers the software-specific slice of risk management (analysis of how software
contributes to hazardous situations, software risk control measures, verification), written as a
specialization of the general process this folder describes. Where a subclause here would otherwise
restate ground module 06 already covers, this folder cites it instead.

**Key areas covered:**
- Scope and applicability of a risk management system to medical device software
- Terms and definitions load-bearing for the rest of the standard
- Establishing and maintaining a risk management process across the device lifecycle
- Management responsibility, personnel competence, the risk management plan, and the risk
  management file

---

## §1 Scope

ISO 14971 applies to a manufacturer's risk management activities across the entire lifecycle of a
medical device — before, during, and after production — independent of the device's underlying
technology. It does not specify what level of risk is acceptable for a given device (that is left to
the manufacturer, informed by applicable regulation and the state of the art), nor does it prescribe
a software development process (that is IEC 62304's role for the software slice).

MduX-rust is a software development kit, not a finished medical device, and this scope clause
applies to a manufacturer's *use* of MduX-rust inside a device's own risk management file, not to
MduX-rust as a shrink-wrapped product with its own risk file. `docs/regulatory-compliance.md` is
explicit about this boundary: MduX-rust supplies "engineering scaffolding," and nothing in this
corpus should be read as MduX-rust itself having undergone, or needing, a device-level risk
management process.

## §2 Normative references

ISO 14971 sits alongside ISO 13485 (the quality management system the risk management process
operates inside) and, for medical device software specifically, IEC 62304 (which folds a
software-specific risk analysis into its own §7 rather than treating risk management as a
freestanding activity for software). See `docs/iso13485/README.md` once populated, and
[`docs/iec62304/06-risk-management-process.md`](../iec62304/06-risk-management-process.md) — the
latter is already written and is the authoritative MduX-rust reference for how software-level hazard
analysis connects to `mdux-governance`'s types; this corpus does not duplicate it.

## §3 Terms and definitions

Two distinctions matter most for how `mdux-governance` and this corpus are structured:

- **Hazard vs. hazardous situation** — a hazard is a potential source of harm in the abstract (for
  example, a UI update that could in principle be delayed); a hazardous situation is a circumstance
  in which a person, property, or the environment is actually exposed to that hazard (for example, a
  clinician relying on a sedation-index display during the specific window that a delayed update
  would matter). `mdux_governance::Hazard` (`crates/mdux-governance/src/lib.rs`) models the hazard
  side of this pair; `docs/iso14971/schemas/risk-record.schema.json`'s `hazardous_situation` field is
  where the more specific, situational half is recorded per risk record — see module 02 §5.4.
- **Risk vs. residual risk** — risk is the combination of the probability of harm and its severity,
  estimated before any control measure is credited; residual risk is what remains after risk control
  measures (module 03 §7) have been implemented. `docs/iso14971/schemas/risk-record.schema.json`
  keeps a single `residual_risk_acceptable` field per record rather than separate pre- and
  post-control risk objects — see module 03 §7.4 for why that is a deliberate simplification, not an
  attempt to model the standard's full state machine.

## §4 General requirements for risk management system

### §4.1 Risk management process

A manufacturer establishes, documents, and maintains a risk management process that runs for the
device's entire lifecycle, not just its initial development. `mdux_governance::ComplianceProgram`
(`crates/mdux-governance/src/lib.rs`) gives an application a running, sequenced record of that
process's outputs — every `add_hazard`, `add_requirement`, and `add_verification` call appends an
`AuditEvent` — but a `ComplianceProgram` instance as constructed in the example applications
(`examples/hello_world`, `examples/class_c_monitor`) is populated once at startup for a given build,
not continuously operated across a device's fielded lifetime the way a real risk management process
is. This mirrors `docs/regulatory-compliance.md`'s own framing: the governance types are scaffolding
for *recording* a process's outputs in a structured, exportable form, not an operating instance of
the process itself.

### §4.2 Management responsibilities

Assigning authority for risk management decisions, defining the organization's risk acceptability
policy, and providing adequate resources for the process are organizational obligations that sit
entirely inside a manufacturer's quality management system. MduX-rust provides no scaffolding here —
`SafetyClass::{B,C}` and `DeviceContext` record a technical classification decision, not who is
accountable for making it or what policy governs it, and nothing in this repository can substitute
for the accountable-person sign-off this subclause requires.

### §4.3 Competence of personnel

Ensuring that everyone performing risk management tasks is competent for the assigned role — through
qualification, training, and experience — is a personnel and QMS record-keeping matter with no
software-engineering component MduX-rust could plausibly automate. This is entirely the
manufacturer's responsibility; no type in `mdux-governance` records who performed an analysis or what
qualified them to do so.

### §4.4 Risk management plan

The risk management plan defines, for a specific device, the scope of the risk management activities
to be performed, assigns responsibilities, sets the criteria for risk acceptability, and describes
verification and review activities. `mdux_governance::ComplianceProgram::new(device: DeviceContext)`
(`crates/mdux-governance/src/lib.rs`) gives a scope-bearing anchor — a `DeviceContext` names the
product, the software item, and its safety class — but the plan's substantive content (acceptability
criteria, review cadence, assigned responsibilities) is not represented by any MduX-rust type today.
`docs/iso14971/schemas/risk-record.schema.json`'s `residual_risk_acceptable` boolean is only
meaningful once such criteria exist somewhere in the manufacturer's own plan; the schema records the
*outcome* of applying the criteria, not the criteria themselves.

### §4.5 Risk management file

The risk management file is the collected set of records that demonstrates the risk management
process was actually carried out for a given device, across its lifecycle. `ComplianceProgram`'s
export methods — `trace_rows()`/`trace_matrix_export()` (requirement-to-verification-to-hazard
traceability) and `audit_export()` (a sequenced, timestamped-by-sequence-number record of every
requirement, hazard, verification, and problem report registered) — are structured, machine-generated
candidates for feeding such a file, and a collection of
`docs/iso14971/schemas/risk-record.schema.json` instances would be another. Neither constitutes the
file itself: `software_development_file/regulatory/ISO_14971/` is where a manufacturer's own
filled-in risk management file belongs (see the [README](README.md)), and it is currently unpopulated
in this repository — MduX-rust supplies inputs a file could draw on, not a completed file.

---

## Related documents

- [Risk analysis and evaluation](02-risk-analysis-and-evaluation.md)
- [Risk control and residual risk](03-risk-control-and-residual-risk.md)
- [Production and post-production activities](04-production-and-post-production.md)
- [IEC 62304 software risk management process](../iec62304/06-risk-management-process.md)
- [AI-Reference index](AI-Reference.md)
- [Citation convention](../governance/citation-convention.md)
