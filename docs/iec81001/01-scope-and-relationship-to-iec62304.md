# IEC 81001-5-1: Scope and relationship to IEC 62304

## Module overview

This module covers what IEC 81001-5-1:2021 applies to, the terms it introduces that matter for
MduX-rust, and — most importantly for a corpus that already has an IEC 62304 module — how the two
standards' processes relate. IEC 81001-5-1 does not describe a parallel life cycle; it describes a
set of security-specific activities that a manufacturer weaves into the life cycle IEC 62304 already
requires, drawing on IEC 62443-4-1's secure-product-development requirements for the "how." Everything
in modules 02-04 assumes this integration model rather than treating security as a separate process
running alongside development.

**Key areas covered:**
- Scope and applicability to health software
- Terms load-bearing for the rest of this module group
- Relationship to IEC 62304's life cycle processes
- Relationship to IEC 62443-4-1's secure-product-development requirements
- What "integration" means concretely for a project like MduX-rust

---

## §4 (approx.) Scope

IEC 81001-5-1 applies to the security of software used in, or as, a health software product across
its life cycle — development, deployment, and maintenance. Like IEC 62304's own scope statement
(see [`../iec62304/01-scope-and-general-requirements.md §1`](../iec62304/01-scope-and-general-requirements.md#1-scope)),
this is a standard aimed at a manufacturer's process for *their* product; MduX-rust is again in the
position of being scaffolding a manufacturer's device software depends on, not the regulated health
software product itself. A manufacturer building on MduX-rust applies IEC 81001-5-1 to their own
device, informed by what MduX-rust's own trust-zone architecture and evidence pipeline already give
them — which is the throughline of modules 02-04.

It is worth being explicit about what "cybersecurity" means in this context versus what it does not:
IEC 81001-5-1 is about the security of the software product's own life cycle and the resulting
artifact — secure design, secure coding practices, vulnerability handling, secure update delivery —
not about network protocol implementations, cryptographic library selection, or hospital IT network
segmentation, which are largely a deployment-environment and systems-integration concern outside a
single SDK's scope. MduX-rust today has **no network stack in any crate or adapter** — no
`adapters/` crate performs networking, and no governed crate parses untrusted network input. Several
activity groups this corpus describes (most notably security update *delivery* mechanisms) are
therefore honestly out of scope for what MduX-rust ships today; module 04 states this explicitly
rather than describing a mechanism that does not exist.

## §4 (approx.) Terms

Two terms matter most for how this module group is organized:

- **Security risk** — the standard treats this as a related but distinct concept from the safety
  risk IEC 62304 §7 / ISO 14971 already require a manufacturer to manage. A safety hazard asks "how
  could this software cause harm through malfunction or foreseeable misuse?"; a security risk asks
  "how could a threat actor's deliberate action cause the software to malfunction, leak data, or
  behave unexpectedly?" The two can share a controlling requirement (a control that prevents
  unauthorized modification of a dosage value is both a safety risk control and a security risk
  control), but they are not the same analysis, and a security review that only re-runs a safety
  hazard analysis under a new name misses attacker-driven scenarios a safety analysis does not
  consider. Module 02 develops this distinction further.
- **Secure product development life cycle** — IEC 81001-5-1's own scope is largely a health-software
  specialization of IEC 62443-4-1's process requirements for how a *product* (not just a specific
  device) is developed securely: security requirements definition, secure design, secure
  implementation, security verification, security update management, and security guidelines
  documentation. These six activity groups are the organizing structure for modules 02-04 of this
  corpus, in preference to guessing at IEC 81001-5-1's own internal sub-clause numbering (see the
  caveat in [`README.md`](README.md)).

## §5 (approx.) Relationship to IEC 62304's life cycle processes

IEC 81001-5-1 is designed to be *integrated* into IEC 62304's process clauses, not run as a separate
parallel program with its own planning, review, and release gates. Concretely, and by analogy with
how IEC 62304 §7 (software risk management) sits inside a manufacturer's overall ISO 14971 process
(see [`../iec62304/06-risk-management-process.md §7.1`](../iec62304/06-risk-management-process.md#71-analysis-of-software-contributing-to-hazardous-situations)):

- Security risk management (module 02) sits alongside IEC 62304 §7's safety risk management, sharing
  the same requirement-and-verification machinery in `mdux-governance` where a control happens to
  serve both purposes, but producing its own distinct risk records where it does not.
- Secure design and secure implementation (module 03) sit alongside IEC 62304 §5.3-§5.5's
  architectural design, detailed design, and unit implementation/verification clauses — a security
  design principle like "least privilege" or "minimize attack surface" is a design constraint applied
  during the same architectural-design activity IEC 62304 §5.3 already requires, not a separate design
  pass.
- Security verification (module 04) sits alongside IEC 62304 §5.5-§5.7's unit/integration/system
  testing — security test cases are additional test cases in the same verification program, tagged by
  what they verify, not a disjoint test suite with its own release gate.
- Security update management (module 04) sits alongside IEC 62304 §6's maintenance process — a
  security patch is a modification like any other, analyzed and re-verified through the same
  maintenance-process machinery (see
  [`../iec62304/05-maintenance-process.md`](../iec62304/05-maintenance-process.md)), with the
  distinguishing feature that its motivating "problem" is a vulnerability report rather than a
  functional defect report.

`mdux-governance`'s `Requirement`/`VerificationCase`/`ProblemReport`/`AuditEvent` types (module 02 of
`../iec62304/`) do not currently distinguish a security-motivated record from a safety-motivated one
at the type level — both flow through the same `ComplianceProgram`. This is an accurate description
of the current repository state, not a design recommendation either way; a manufacturer wanting a
sharper distinction would extend these types (e.g. a `category` field) rather than finding one
already built in.

## §5 (approx.) Relationship to IEC 62443-4-1

IEC 62443-4-1 is an industrial/general-purpose secure-product-development-life-cycle standard;
IEC 81001-5-1 is best understood as adapting its requirements to the health-software domain and
integrating them with IEC 62304 as described above, rather than introducing a wholly new set of
activities from scratch. This corpus does not maintain a separate module for IEC 62443-4-1 itself —
where a control in modules 02-04 below traces conceptually back to a IEC 62443-4-1-style
secure-development practice (e.g. a documented secure coding standard, or a threat-modeling step
before design sign-off), that lineage is noted in prose rather than given its own citation key, since
this corpus's stated scope (see `../governance/citation-convention.md`) is the five standards listed
there, of which IEC 62443-4-1 is not one.

---

## Related documents

- [Security risk management](02-security-risk-management.md)
- [Secure design and implementation](03-secure-design-and-implementation.md)
- [Security verification and update management](04-security-verification-and-update-management.md)
- [IEC 62304 corpus](../iec62304/README.md)
- [AI-Reference index](AI-Reference.md)
- [Citation convention](../governance/citation-convention.md)
