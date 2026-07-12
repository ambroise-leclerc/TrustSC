# IEC 81001-5-1: Security risk management

## Module overview

This module covers the security risk management activity group: identifying threats and
vulnerabilities that could affect a health software product, distinguishing that analysis from the
safety hazard analysis IEC 62304 §7 already requires, and connecting a security risk to whatever
control actually mitigates it. The relationship between security risk and safety risk is the single
most important idea in this module — the two overlap in places but are not interchangeable, and
treating them as the same analysis under two names is a common and consequential mistake.

**Key areas covered:**
- Identifying threats and vulnerabilities (as distinct from hazards)
- Why security risk and safety risk are not the same analysis, and where they connect
- Security risk assessment and acceptance
- Mapping security risk controls back to MduX-rust's own architecture

---

## §5 (approx.) Identifying threats and vulnerabilities

A safety hazard analysis (IEC 62304 §7.1, see
[`../iec62304/06-risk-management-process.md §7.1`](../iec62304/06-risk-management-process.md#71-analysis-of-software-contributing-to-hazardous-situations))
asks how software could contribute to a hazardous situation through malfunction, incorrect output, or
foreseeable misuse. A security risk analysis asks a different question: how could a threat actor
deliberately cause the software to malfunction, disclose data it shouldn't, or be manipulated into a
state its designers never intended? The inputs to this analysis are threats (an actor and a
capability) and vulnerabilities (a weakness the threat could exploit), not failure modes.

For MduX-rust, this analysis is scoped by what actually exists to attack. The governed crates
(`crates/`) are `#![forbid(unsafe_code)]` and take no untrusted network input; the presentation
adapter (`adapters/mdux-vulkan-winit`) reads local, committed shader/glyph-atlas evidence and local
window-system events, not remote input; the host tooling (`tools/`) runs offline against
manufacturer-controlled inputs. This is a materially smaller and more static attack surface than a
networked device, and a manufacturer's threat identification exercise should reflect that rather than
importing a generic web-application threat model wholesale. It does not mean the exercise is
unnecessary — a manufacturer's own additions (a networked adapter, a file-import feature, a
Bluetooth peripheral) each reopen this analysis for the surface they add.

## §5 (approx.) Security risk is not safety risk

The two analyses can share a controlling requirement — a requirement that authenticates a firmware
update before applying it is simultaneously a security risk control (prevents a malicious update from
being installed) and, indirectly, a safety risk control (prevents the device from running unverified,
potentially unsafe code). But most security risks have no safety-risk analogue at all: a
confidentiality breach that discloses patient data without ever affecting device function is a
security risk with essentially zero safety-risk weight, and a safety hazard like a rendering
race condition that never involves an adversary is a safety risk with no security dimension.
`mdux_governance::Hazard` (module 06 of `../iec62304/`) models the safety side of this — a hazard with
at least one controlling requirement — and is not a security risk record by another name; it has no
field for a threat actor, an attack vector, or a confidentiality/integrity/availability impact
classification, which is exactly why this corpus introduces a separate schema
([`schemas/security-risk-record.schema.json`](schemas/security-risk-record.schema.json)) rather than
overloading `Hazard`.

The practical consequence for a manufacturer: running only a safety hazard analysis and calling it
"security review complete" leaves deliberate-actor scenarios unexamined. Conversely, running a
security review without connecting a found control back to the safety risk register can miss that a
security control is also load-bearing for patient safety (as in the firmware-authentication example
above), which matters for how rigorously that control itself gets verified and change-controlled.

## §5 (approx.) Security risk assessment and acceptance

Once a threat/vulnerability pair is identified, it needs an impact assessment (what would happen if
exploited — using the classic confidentiality/integrity/availability framing this module's schema
adopts) and a decision on whether the residual risk, after whatever controls exist, is acceptable.
This mirrors ISO 14971's accept/mitigate/transfer structure for safety risk, applied to a different
kind of risk with different inputs. `docs/iec81001/schemas/security-risk-record.schema.json`'s
`residual_risk_acceptable` boolean is a deliberately blunt instrument — a real security risk file
would carry a fuller rationale, likelihood/severity scoring, and a review/approval trail — but it
gives a manufacturer a machine-checkable place to record the acceptance decision itself, matching how
`docs/iec62304/schemas/safety-classification.schema.json`'s `approval` object records a safety
classification's sign-off.

## §5 (approx.) Where MduX-rust's architecture already functions as a control

Several MduX-rust mechanisms documented elsewhere in this corpus for other reasons double as security
risk controls, and a manufacturer's security risk record can cite them directly rather than
re-justifying the same property from scratch:

- The governed/adapter/tools trust-zone split (ADR-005) confines every third-party dependency and all
  `unsafe` code to a small, named set of crates — reducing the software's attack surface to what is
  reachable from those crates, and making "which code could an attacker's input actually reach" an
  answerable question rather than a whole-dependency-graph audit. See module 03 for this as a secure
  design control in its own right.
- The SOUP register (`docs/governance/soup-register.toml`) is a dependency-provenance record — for
  security risk purposes, it is also the starting point for a supply-chain / vulnerable-dependency
  assessment, since it already lists every third-party crate, its supplier, its exact pinned version,
  and which trust zone confines it. It does not currently track upstream-reported CVEs against those
  versions (the same gap `../iec62304/07-configuration-management-process.md §8.3.2` notes for SOUP
  anomalies generally) — a manufacturer's security risk process should treat that as a process they
  run against the register's contents, not something the register does automatically today.
- Byte-verified generated evidence (ADR-007, `bake`/`verify`) is an integrity control on the build
  pipeline itself: it detects an artifact (a font atlas, a shader, an ML model package) that has
  drifted from what a reviewed source input and recipe would deterministically produce, whether that
  drift is accidental or the result of tampering. Module 03 covers this in more depth as a supply-
  chain integrity control.

---

## Related documents

- [Scope and relationship to IEC 62304](01-scope-and-relationship-to-iec62304.md)
- [Secure design and implementation](03-secure-design-and-implementation.md)
- [IEC 62304 risk management process](../iec62304/06-risk-management-process.md)
- [Security risk record schema](schemas/security-risk-record.schema.json)
- [AI-Reference index](AI-Reference.md)
- [Citation convention](../governance/citation-convention.md)
