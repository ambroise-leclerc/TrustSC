# IEC 62304: Scope and general requirements

## Module overview

This module covers IEC 62304:2006+AMD1:2015's front matter and its cross-cutting general
requirements clause: what the standard applies to, the terms it defines, and the four
foundational obligations (quality management system integration, risk management integration,
software safety classification, and legacy software provisions) that every later process clause
assumes is already in place. Everything in modules 02-08 is scoped by the classification decision
made here.

**Key areas covered:**
- Scope and applicability to medical device software
- Terms and definitions load-bearing for the rest of the standard
- Quality management system and risk management integration points
- Software safety classification (Class A / B / C)
- Legacy software provisions

---

## §1 Scope

IEC 62304 applies to the development and maintenance of medical device software, whether the
software is itself the medical device, embedded in one, or an accessory to one. It does not
prescribe a quality management system in its own right (that's ISO 13485's job) or a specific
risk management methodology (that's ISO 14971's) — it assumes both exist and defines the
software-specific lifecycle processes that sit inside them.

TrustSC is a software development kit, not a finished medical device — clause 1's scope applies
to a manufacturer's *use* of TrustSC inside their own device software, not to TrustSC as a
shrink-wrapped product. `docs/regulatory-compliance.md` states this explicitly: TrustSC provides
"engineering scaffolding," not the manufacturer's own IEC 62304 process artifacts.

## §2 Normative references

IEC 62304 is normatively tied to ISO 13485 (quality management), ISO 14971 (risk management), and
IEC 62366-1 (usability engineering) for a device with a user interface — precisely the four
standards this corpus documents (plus IEC 81001-5-1 for cybersecurity, which post-dates 62304's
2006 baseline and is referenced informatively rather than normatively by it). See
`docs/iso13485/README.md`, `docs/iso14971/README.md`, `docs/iec62366/README.md`, and
`docs/iec81001/README.md`.

## §3 Terms and definitions

Two definitions matter most for how this corpus and `trustsc-governance` are structured:

- **SOUP** (Software Of Unknown Provenance) — software not developed to comply with IEC 62304 for
  the device at hand. TrustSC tracks SOUP explicitly in `docs/governance/soup-register.toml`,
  which records supplier, license, integration path, and risk controls per dependency — see
  `docs/iec62304/08-problem-resolution-process.md` and `docs/iec62304/06-risk-management-process.md`
  for how a SOUP entry connects to hazard analysis.
- **Software item / software unit / software system** — the standard's hierarchy of decomposition.
  TrustSC's own trust-zone split (`crates/` governed, `adapters/` edge, `tools/` host-only — ADR-005)
  is a segregation of software items by trust boundary, which is directly relevant to
  [§5.3.3's segregation provisions](03-development-design.md#533-identify-segregation-necessary-for-risk-control)
  (module 03), not to §4.4 below (which covers legacy software).

## §4 General requirements

### §4.1 Quality management system

A manufacturer's software development process runs inside their ISO 13485 QMS. TrustSC does not
provide that QMS — `docs/iso13485/README.md` and `software_development_file/regulatory/ISO_13485/README.md`
describe precisely which pieces of scaffolding this project contributes toward one and which the
manufacturer must still supply.

### §4.2 Risk management process

IEC 62304 §7 (this corpus's module 06) is itself the software-specific slice of a manufacturer's
overall ISO 14971 risk management process — the two are not separate activities. `trustsc-governance`'s
`Hazard` type, which requires at least one controlling `Requirement` (`Hazard::validate()`), is a
machine-checked expression of this integration: a hazard recorded without a software requirement
that controls it fails validation, matching §4.2's expectation that risk control measures are
expressed as software requirements, not free-floating mitigations.

### §4.3 Software safety classification

The standard defines three classes by the *worst-case* severity of harm the software could
contribute to, assessed before risk controls are applied:

- **Class A** — no injury or damage to health is possible.
- **Class B** — non-serious injury is possible.
- **Class C** — death or serious injury is possible.

`trustsc_core::SafetyClass` models only `B` and `C` — TrustSC's UI runtime, ML inference engine, and
governance types are built for devices where software failure is at minimum a non-serious-injury
risk, and the framework's evidence-generation rigor (deterministic rendering, fail-closed ML
self-test, byte-verified asset pipelines) is calibrated to that floor. A manufacturer whose
component is genuinely Class A does not need `trustsc-governance`'s enforcement and can use a lighter
process; nothing in this corpus should be read as implying TrustSC supports or was validated for
a Class A classification path. `docs/iec62304/schemas/safety-classification.schema.json` reflects
this by restricting `overall_classification` to `Class_B`/`Class_C`.

`ComplianceProgram::validate()` (`crates/trustsc-governance/src/lib.rs`) enforces one classification
consequence directly: a `DeviceContext` with `safety_class == SafetyClass::C` must have at least
one recorded `Hazard`, or validation fails. Class B carries no such minimum — matching §4.3's
principle that classification drives the rigor of everything downstream.

### §4.4 Legacy software

Not applicable to TrustSC today — the project has no shipped, previously-unclassified software
line being brought under IEC 62304 retroactively. A manufacturer integrating an older UI or ML
component alongside TrustSC would apply §4.4's gap-analysis provisions to that component, not to
TrustSC itself.

---

## Related documents

- [Development planning and requirements](02-development-planning-and-requirements.md)
- [Development design](03-development-design.md)
- [Implementation and testing](04-development-implementation-and-testing.md)
- [Maintenance process](05-maintenance-process.md)
- [Risk management process](06-risk-management-process.md)
- [Configuration management process](07-configuration-management-process.md)
- [Problem resolution process](08-problem-resolution-process.md)
- [AI-Reference index](AI-Reference.md)
- [Citation convention](../governance/citation-convention.md)
