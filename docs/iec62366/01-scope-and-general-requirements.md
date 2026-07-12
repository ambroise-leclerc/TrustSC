# IEC 62366-1: Scope and general requirements

## Module overview

This module covers IEC 62366-1:2015+AMD1:2020's front matter and its cross-cutting general
requirements clause: what the standard applies to, the terms load-bearing for the rest of the
corpus, how the standard is scoped when a user interface predates the usability engineering
process being applied to it, and the usability engineering file itself — the single record that
every later clause in modules 02-03 contributes evidence to. Where IEC 62304 organizes a software
lifecycle around classification, IEC 62366-1 organizes a usability engineering process around one
question: could a use error, not a software defect, contribute to harm?

**Key areas covered:**
- Scope and applicability to medical device user interfaces
- Terms and definitions load-bearing for the rest of the standard
- How the standard applies to an existing/legacy user interface
- The usability engineering file and what it collects

---

## §1 Scope

IEC 62366-1 applies to the process a manufacturer runs to make a medical device's user interface
safe with respect to *use*, not just to its individual functions being implemented correctly. A
device can pass every IEC 62304 verification case and still be unsafe if an operator can
misread a display, press the wrong control under stress, or misinterpret an alarm state — that
class of failure is this standard's subject, and it sits alongside, not inside, IEC 62304's
software lifecycle (see `docs/iec62304/01-scope-and-general-requirements.md §2 Normative
references`).

TrustSC is a UI software development kit, not a finished medical device, and it does not
author a device's intended use, its user population, or its use environment — those are the
manufacturer's inputs to clause 5.1 below. What TrustSC does provide is a constrained UI
authoring and rendering layer (the MedUI DSL, described throughout module 02) whose compile-time
checks and runtime determinism narrow a specific, real slice of use-related risk: text that
overflows or truncates, controls that overlap or render in the wrong place, and safety-critical
elements whose approved content silently drifts from what shipped. This corpus is honest that
this is a narrow slice — clause 1's full scope (use specification, formative and summative
evaluation with real users, clinical-workflow risk analysis) is substantially broader than
anything a UI SDK can automate, and modules 02-03 say so at each point where TrustSC has
nothing to offer.

## §2 Normative references

IEC 62366-1 is normatively tied to ISO 14971 (risk management) — a hazard-related use scenario
identified under this standard's process is a risk management input, not a parallel risk
process — and is referenced *by* IEC 62304 §2 as the usability-relevant standard for a device
with a user interface. See `docs/iso14971/README.md` and `docs/iec62304/01-scope-and-general-requirements.md
§2 Normative references`. Where IEC 62304 asks "does the software do what it was designed to
do," IEC 62366-1 asks "can the intended user actually operate it correctly," and a manufacturer's
technical file needs both answers, cross-referenced rather than treated as interchangeable
evidence.

## §3 Terms and definitions

A handful of terms drive how this corpus and TrustSC's mechanisms line up:

- **User interface** — every means by which an operator and the device exchange information,
  including controls, displays, alarms, and packaging/labeling as far as it affects operation.
  TrustSC's slice of this is the on-device rendered surface only: the compiled screens produced
  by the MedUI DSL (`docs/dsl/overview.md`) and rendered by `adapters/trustsc-vulkan-winit`. Physical
  controls, packaging, and instructions-for-use fall outside what this SDK touches.
- **Use error** — an act or omission by a user that produces a result different from what the
  manufacturer intended or the user expected, distinguished from **abnormal use** (a user action
  or inaction reasonably excluded from risk control because it contradicts foreseeable use).
  TrustSC cannot classify a real operator's action as a use error or abnormal use — that
  judgment requires the manufacturer's use specification (§5.1) and clinical context. What
  TrustSC's `--verify-ui` tooling (ADR-016, module 03) can do is confirm that the interface, as
  rendered, presents the information the manufacturer intended it to present, which is a
  precondition for a use-error analysis to be meaningful at all.
- **Hazard-related use scenario** — a use scenario whose associated use error(s) could lead to
  a hazardous situation. This is the concept module 02's §5.2 discussion is built around, and the
  concept the `usability-engineering-record.schema.json` (`schemas/`, this folder) models directly
  via its `use_scenario.hazard_related` boolean.
- **Usability engineering file** — the standard's equivalent of a design history file, specific to
  usability: every use specification, evaluation plan, formative and summative evaluation record,
  and the resulting user interface specification, collected so a reviewer can trace a shipped
  interface back to the evidence that justified it. §4.2 below describes what TrustSC
  contributes toward one.
- **Formative evaluation** / **summative evaluation** — module 03 covers both; the short version
  is that formative evaluation informs design iteratively during development, while summative
  evaluation confirms the finished interface is safe for its intended users, uses, and
  environments, typically through observed use by representative users. TrustSC has real,
  running mechanisms relevant to formative evaluation and essentially nothing for summative
  evaluation — see module 03 for exactly where that line falls.

## §4 General requirements

### §4.1 Application of this standard

The usability engineering process in clause 5 (modules 02-03) applies to a device's user
interface as a whole, but the standard explicitly addresses the case where part or all of that
interface already exists before the process is formally applied to it — a **user interface of
unknown provenance** — mirroring IEC 62304 §4.4's treatment of legacy software (`docs/iec62304/01-scope-and-general-requirements.md
§4.4 Legacy software`). In that case the standard permits a gap analysis against available
field/complaint/service history and prior evaluation records, rather than requiring the full
process to be repeated from a blank slate, provided the gap analysis itself is documented and any
identified gap is closed.

TrustSC has no code path for this today, and this corpus does not pretend otherwise. There is
no mechanism for importing an existing, non-MedUI-authored screen and attaching retrofitted
usability evidence to it — a manufacturer bringing a legacy UI (built before adopting TrustSC,
or built outside it entirely) under this standard applies §4.1's gap-analysis provisions using
their own process, and the resulting record lives in their usability engineering file, not in
anything this SDK generates. Where §4.1 is directly actionable is the opposite direction: every
screen compiled by the MedUI DSL is, by construction, newly authored against a known, inspectable
specification (`docs/dsl/overview.md`) — there is no ambiguity about its provenance to resolve,
because the compiler that produced it and the source `.medui` file that describes it are both
part of the same build.

A related point worth stating plainly: nothing about compiling a screen through the MedUI DSL
*substitutes* for applying this standard's process to it. A `.medui`-authored, `--verify-ui`-passing
screen is evidence that the interface renders what was specified — it is not evidence that what
was specified is usable, safe, or appropriate for its intended users. Clause 5's process (module
02) is what establishes that; module 02 and 03 return to this distinction repeatedly.

### §4.2 Usability engineering file

The usability engineering file collects the use specification, the identified hazard-related use
scenarios, the user interface specification, the evaluation plan, and the formative/summative
evaluation records into one traceable set, so a reviewer can follow a shipped interface's design
back to the analysis that justified it — the usability counterpart of IEC 62304's overall design
history file discipline. TrustSC does not assemble this file; it is squarely the manufacturer's
document, built around their own use specification and evaluation program.

What this corpus and `trustsc-governance` contribute is a structured place to *record* two pieces
that belong inside that file once a manufacturer has done the underlying work: a hazard-related
use scenario, its evaluation, and its findings (`schemas/usability-engineering-record.schema.json`,
this folder — an entry per use scenario/evaluation pairing, deliberately mirroring the shape of
`docs/iec62304/schemas/hazard.schema.json` and `verification-case.schema.json`), and the risk
control measures that scenario drove, which — like every risk control measure under IEC 62304
§5.2.2 (`docs/iec62304/02-development-planning-and-requirements.md §5.2.2`) — should exist as an
actual `trustsc_governance::Requirement`, not a free-floating note. `risk_control_measures` in that
schema is deliberately a list of strings intended to hold real `RequirementId`s for exactly this
reason. None of this constitutes an operating usability engineering file by itself — see
`docs/regulatory-compliance.md`'s "Governance types are scaffolding, not an operating QMS"
section, which applies here verbatim.

---

## Related documents

- [Use specification and user interface design](02-use-specification-and-ui-design.md)
- [Formative and summative evaluation](03-formative-and-summative-evaluation.md)
- [AI-Reference index](AI-Reference.md)
- [Citation convention](../governance/citation-convention.md)
- [IEC 62304 corpus](../iec62304/README.md)
