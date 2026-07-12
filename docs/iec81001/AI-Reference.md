> **Clause-numbering caveat (read this first).** IEC 81001-5-1's exact sub-clause numbering is
> reproduced here with **lower confidence** than the IEC 62304 / ISO 13485 / ISO 14971 / IEC 62366-1
> material elsewhere in this corpus. Every section reference below (`§4`, `§5`, `§6`, `§7`, ...) is a
> **broad, approximate placeholder** organized around the standard's known real activity groups —
> SECURITY RISK MANAGEMENT, SECURE DESIGN, SECURE IMPLEMENTATION, SECURITY VERIFICATION, MANAGEMENT OF
> SECURITY-RELATED ISSUES / SECURITY UPDATE MANAGEMENT, SECURITY GUIDELINES — not a verified clause
> tree. **Do not cite a `§` reference from this file, or from `docs/iec81001/*.md` generally, in a
> real regulatory submission or `Justification` object without first checking it against your own
> licensed copy of the current edition of IEC 81001-5-1.** This is a stronger caveat than applies to
> the other four standards in this corpus — see `docs/iec81001/README.md`'s opening paragraph for the
> same statement in full.

*This is a compact, complete index over `docs/iec81001/`'s modular files — one row per activity
group, not a parallel transcription. It contains no reproduced standard text (see
[`../governance/citation-convention.md`](../governance/citation-convention.md)); use it to find which
module covers a given activity group, then open that module for the actual explanatory prose and
MduX-rust cross-references.*

# IEC 81001-5-1:2021 — AI-Reference index

## §4-§5 (approx.) — Scope and relationship to IEC 62304 ([detail](01-scope-and-relationship-to-iec62304.md))

- **Scope** — health software product life cycle security; MduX-rust is again scaffolding a
  manufacturer's device software uses, not itself the regulated product; MduX-rust has no network
  stack in any crate today. [→](01-scope-and-relationship-to-iec62304.md#4-approx-scope)
- **Terms** — security risk vs. safety risk (distinct concepts); secure product development life
  cycle as six activity groups. [→](01-scope-and-relationship-to-iec62304.md#4-approx-terms)
- **Relationship to IEC 62304's life cycle** — security activities integrate into existing IEC 62304
  §5/§6/§7 process clauses rather than running as a parallel program. [→](01-scope-and-relationship-to-iec62304.md#5-approx-relationship-to-iec-62304s-life-cycle-processes)
- **Relationship to IEC 62443-4-1** — IEC 81001-5-1 as a health-software adaptation of IEC 62443-4-1's
  secure-product-development requirements; not separately modularized in this corpus. [→](01-scope-and-relationship-to-iec62304.md#5-approx-relationship-to-iec-62443-4-1)

## §5 (approx.) — Security risk management ([detail](02-security-risk-management.md))

- **Identifying threats and vulnerabilities** — distinct from hazard identification; MduX-rust's
  attack surface is scoped by the trust-zone boundary and the absence of a network stack. [→](02-security-risk-management.md#5-approx-identifying-threats-and-vulnerabilities)
- **Security risk is not safety risk** — the two can share a controlling requirement but are
  different analyses; `Hazard` is not a security risk record. [→](02-security-risk-management.md#5-approx-security-risk-is-not-safety-risk)
- **Security risk assessment and acceptance** — impact framing (confidentiality/integrity/
  availability) and a residual-risk acceptance decision, mirrored in the schema below. [→](02-security-risk-management.md#5-approx-security-risk-assessment-and-acceptance)
- **Architecture as an existing control** — trust-zone boundary, SOUP register, byte-verified
  evidence as controls a manufacturer's security risk record can cite directly. [→](02-security-risk-management.md#5-approx-where-mdux-rusts-architecture-already-functions-as-a-control)

## §6 (approx.) — Secure design and secure implementation ([detail](03-secure-design-and-implementation.md))

- **Secure design principles** — attack surface minimization, defense in depth, least privilege,
  applied to MduX-rust's real architecture rather than stated abstractly. [→](03-secure-design-and-implementation.md#6-approx-secure-design-principles)
- **Trust-zone boundary as attack-surface minimization** — ADR-005's governed/adapter/tools split;
  ADR-012's "owned data only" rule at the adapter boundary. [→](03-secure-design-and-implementation.md#attack-surface-minimization-via-the-trust-zone-boundary)
- **`#![forbid(unsafe_code)]`** — compiler-enforced memory safety across every governed crate; scoped
  honestly (does not cover `adapters/`/`tools/`). [→](03-secure-design-and-implementation.md#6-approx-forbidunsafe_code-as-a-secure-implementation-control)
- **Secure implementation practices** — input validation at package/model construction
  (`TextRuntime::new()`, `Classifier1D::new()`), strictly-ordered deterministic arithmetic, byte-
  verified build-pipeline integrity (ADR-007). [→](03-secure-design-and-implementation.md#6-approx-secure-implementation-practices-beyond-memory-safety)
- **Honest limits** — what the trust-zone boundary and `forbid(unsafe_code)` do *not* guarantee. [→](03-secure-design-and-implementation.md#6-approx-honest-limits)

## §6-§7 (approx.) — Security verification and update management ([detail](04-security-verification-and-update-management.md))

- **Security verification** — additional test cases inside the existing `VerificationCase`
  machinery; `Classifier1D::new()`'s fail-closed self-test and `bake`/`verify` CI as existing
  security-relevant checks; static analysis / dependency scanning flagged as a gap. [→](04-security-verification-and-update-management.md#6-approx-security-verification)
- **Management of security-related issues** — `ProblemReport` as the existing intake/resolution type;
  no built-in security/safety category distinction today. [→](04-security-verification-and-update-management.md#7-approx-management-of-security-related-issues)
- **Security update management** — honestly not yet applicable for update *delivery*: no crate in the
  workspace performs networking; update authentication/delivery/rollback is entirely the
  manufacturer's responsibility. [→](04-security-verification-and-update-management.md#7-approx-security-update-management)
- **Security guidelines for users** — MduX-rust has no end-user product; developer-facing guidance on
  which mechanisms have security implications a manufacturer's own operator documentation should
  reflect. [→](04-security-verification-and-update-management.md#7-approx-security-guidelines-for-users)

---

For a design decision's justification against this standard, use the `Justification` object in
[`../iec62304/schemas/justification.schema.json`](../iec62304/schemas/justification.schema.json),
citing a `clause_ref` built from the section placeholders above — and, per the caveat at the top of
this file, verify that placeholder against the actual current edition of IEC 81001-5-1 before relying
on it in a real submission.
