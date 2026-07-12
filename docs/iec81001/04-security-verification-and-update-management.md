# IEC 81001-5-1: Security verification and update management

## Module overview

This module covers two further activity groups: security verification (testing/reviewing that secure
design and implementation decisions actually hold) and management of security-related issues /
security update management (finding out about a vulnerability in fielded software and getting a fix
out). It also covers security guidelines — documentation a manufacturer provides to users/operators
about the security aspects of using the product safely. The honest center of this module is that
MduX-rust today has **no network stack anywhere in the workspace**, which makes several sub-activities
of "security update management" as commonly understood (remote vulnerability disclosure intake at
scale, over-the-air patch delivery, field telemetry) not yet applicable — this module says so
explicitly rather than describing a mechanism that does not exist.

**Key areas covered:**
- Security verification activities and how they fit MduX-rust's existing test/evidence machinery
- Management of security-related issues: how a found vulnerability would flow through existing types
- Security update management: what exists today, and what is explicitly the manufacturer's
  responsibility because no network stack exists
- Security guidelines for users/operators

---

## §6 (approx.) Security verification

Security verification activities typically include: verifying that secure design decisions were
actually implemented as intended, targeted testing for known vulnerability classes, static/dynamic
analysis, and dependency/SOUP vulnerability scanning. IEC 81001-5-1's integration model (module 01)
treats these as additional test cases inside the same verification program IEC 62304 §5.5-§5.7
already requires, not a separate security test suite with its own release gate.

`mdux_governance::VerificationCase` (see
[`../iec62304/02-development-planning-and-requirements.md §5.2.4`](../iec62304/02-development-planning-and-requirements.md#524-verify-software-requirements))
has no dedicated "security" category today, but nothing prevents a manufacturer from recording a
security-motivated verification case through the same type — a `VerificationCase` whose
`Requirement` traces to a security risk control (module 02) is exactly as valid a use of the existing
machinery as one tracing to a safety risk control, and `ComplianceProgram::validate()`'s "every
requirement needs ≥1 verification" rule applies uniformly regardless of why the requirement exists.

Two existing MduX-rust mechanisms already function as security-relevant verification even though they
were built for other stated purposes, and a manufacturer's security verification plan can cite them
directly:

- **`Classifier1D::new()`'s fail-closed self-test** (ADR-017) re-runs every baked golden vector at
  construction and refuses to proceed on any bit-mismatch. Framed as security verification rather than
  purely a safety control, this is a runtime integrity check on the deployed model package — it would
  catch a corrupted or substituted `package.json` as readily as a genuine toolchain miscompilation,
  though it was not designed against an adversarial threat model and should not be relied on as the
  sole defense against a deliberately crafted malicious package.
- **`tools/*-baker verify` in CI** (ADR-007) independently re-derives every committed evidence
  artifact's digest and fails the build on mismatch — functionally a build-integrity verification
  step, run on every CI invocation per `CLAUDE.md`'s "Replaying CI locally" section, not just at
  release time.

Static analysis and dependency vulnerability scanning (e.g. `cargo audit` or equivalent) are not
currently wired into this repository's CI as described in `CLAUDE.md`; this is a gap for a
manufacturer's own security verification plan to close, in the same spirit that
[`../iec62304/07-configuration-management-process.md §8.3.2`](../iec62304/07-configuration-management-process.md#832-soup-anomaly-list)
flags SOUP anomaly tracking as a gap rather than an implemented feature.

## §7 (approx.) Management of security-related issues

A discovered vulnerability — whether found internally, reported by a user, or disclosed by a
dependency's maintainers — needs an intake, triage, and resolution path. `mdux_governance::ProblemReport`
(see
[`../iec62304/08-problem-resolution-process.md §9.1`](../iec62304/08-problem-resolution-process.md#91-establish-problem-resolution-process))
is the existing MduX-rust type for recording a problem and its resolution, and its `closed` field
gates `ComplianceProgram::validate()` in the same way a functional defect report does. Nothing in its
current shape distinguishes "this problem report exists because of a security vulnerability" from any
other problem report — a manufacturer wanting that distinction tracked explicitly would extend the
type (e.g. a category or CVE-reference field) rather than finding one already present, mirroring
module 01's note about `Requirement`/`VerificationCase` having no built-in security/safety split
either.

The SOUP register (`docs/governance/soup-register.toml`, introduced for configuration-management
purposes in
[`../iec62304/07-configuration-management-process.md §8.1.3`](../iec62304/07-configuration-management-process.md#813-soup-identification))
is the natural place a manufacturer would connect an upstream dependency's disclosed vulnerability to
the exact pinned version and trust zone it affects — each entry already carries `component_id`,
`version`, and `pinned_by` — but, as module 02 notes, the register does not itself track upstream CVEs
today; that join is a process a manufacturer runs, not a feature the register automates.

## §7 (approx.) Security update management

This is the sub-activity where MduX-rust's current scope boundary matters most. Security update
management, as commonly understood for a fielded networked product, includes: a mechanism to detect
that an update is available, a mechanism to deliver it to the device, a mechanism to authenticate and
apply it safely, and a mechanism to roll back a failed update. **None of these exist in MduX-rust
today, because no crate in `crates/`, `adapters/`, or `tools/` performs networking of any kind** —
`docs/governance/soup-register.toml` has no HTTP client, TLS library, or update-protocol dependency
listed, and `../architecture.md`'s crate map has no networked adapter. This is not an oversight to be
quietly worked around; it is a scope boundary this document states honestly, matching
`docs/regulatory-compliance.md`'s own pattern of stating what the project does not provide rather than
implying a broader claim.

Concretely, this means:

- **Update delivery, authentication, and rollback are entirely the manufacturer's responsibility.**
  MduX-rust's `bake`/`verify` evidence pattern (ADR-007) gives a manufacturer a byte-verified way to
  know exactly what a given generated artifact (a font, shader, or ML model package) *is*, which is
  useful raw material for a manufacturer's own update-authentication scheme (e.g. signing a
  `report.json` digest before shipping it as part of an update), but MduX-rust does not implement
  signing, transport, or a device-side update agent itself.
- **`Classifier1D::new()`'s fail-closed self-test (ADR-017) is the closest thing to an update-safety
  control that exists today** — if a manufacturer's own update mechanism ever delivered a corrupted or
  incompatible model package, the runtime would refuse to start rather than silently classify signals
  incorrectly. This is a device-side safety net, not an update-delivery mechanism.
- If a future adapter crate does add networking (e.g. for telemetry, remote configuration, or update
  delivery), it would need its own ADR per `CLAUDE.md`'s "When adding a dependency, first ask which
  zone the crate lives in" guidance, and this module's "not yet applicable" framing would need to be
  revisited alongside that ADR — this corpus should not be read as permanently ruling that out, only
  as accurately describing today's repository.

## §7 (approx.) Security guidelines for users

IEC 81001-5-1 also expects a manufacturer to provide security-relevant guidance to the people who
operate the product — secure configuration steps, what to do if a compromise is suspected, and the
security implications of any user-facing configuration choices. MduX-rust does not ship an end-user
product, so it has no operator-facing documentation of this kind itself; what it can meaningfully
provide is developer-facing guidance about which of its own mechanisms have security implications a
manufacturer's own user documentation should reflect — for example, that a device's `.medui` screen
files and compiled text/ML packages are build-time, trusted inputs (see
[`../adr/ADR-008-deterministic-medui-dsl-boundary.md`](../adr/ADR-008-deterministic-medui-dsl-boundary.md))
and should be treated as part of the manufacturer's controlled configuration, not as something an
operator should be able to substitute at runtime.
[`software_development_file/regulatory/IEC_81001/Cybersecurity_SAD.md`](../../software_development_file/regulatory/IEC_81001/Cybersecurity_SAD.md)
is MduX-rust's own filled-in cybersecurity SAD document, carrying this guidance in a form closer to a
technical file than this corpus's own developer-facing prose; see
[`software_development_file/templates/IEC_81001/Cybersecurity_SAD.md`](../../software_development_file/templates/IEC_81001/Cybersecurity_SAD.md)
for the blank template a manufacturer starts from.

---

## Related documents

- [Secure design and implementation](03-secure-design-and-implementation.md)
- [Security risk management](02-security-risk-management.md)
- [IEC 62304 problem resolution process](../iec62304/08-problem-resolution-process.md)
- [IEC 62304 maintenance process](../iec62304/05-maintenance-process.md)
- [ADR-017](../adr/ADR-017-zero-soup-ml-inference-pipeline.md)
- [SOUP register](../governance/soup-register.toml)
- [AI-Reference index](AI-Reference.md)
- [Citation convention](../governance/citation-convention.md)
