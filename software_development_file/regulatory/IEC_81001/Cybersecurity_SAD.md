# Cybersecurity Software Architecture Design — MduX-rust

> Filled-in example for MduX-rust itself. See
> [`software_development_file/templates/IEC_81001/Cybersecurity_SAD.md`](../../templates/IEC_81001/Cybersecurity_SAD.md)
> for the blank template, and [`docs/iec81001/README.md`](../../../docs/iec81001/README.md) — note
> that document's caveat about clause-numbering uncertainty for this newer standard.

## Document control

- **Product / software item:** MduX-rust
- **Scope note:** MduX-rust has no network stack anywhere in the workspace today — no crate performs
  network I/O. Several activity groups below (security update delivery in particular) are therefore
  stated as not-yet-applicable rather than described, and a manufacturer adding connectivity on top
  of MduX-rust takes on that scope entirely themselves.

## 1. Scope and relationship to the IEC 62304 lifecycle

Security risk management runs alongside, not instead of, the safety risk management described in
[`software_development_file/regulatory/ISO_14971/Risk_Management_File.md`](../ISO_14971/Risk_Management_File.md) —
a security issue in a governed or adapter crate could also be a safety hazard if it compromises the
UI/ML behavior a device relies on.

## 2. Security risk management

The trust-zone boundary that separates `crates/` (governed), `adapters/` (edge), and `tools/`
(host-only) — see
[`software_development_file/regulatory/IEC_62304/SAD.md §2-4`](../IEC_62304/SAD.md) — is also
MduX-rust's primary security control: it minimizes the amount of code capable of memory-unsafety or
native-handle misuse to a single, narrow, reviewable adapter crate, rather than spreading `unsafe`
surface area across the whole UI/ML stack.
[`docs/governance/soup-register.toml`](../../../docs/governance/soup-register.toml) is this
project's attack-surface/dependency-provenance record: every third-party crate's supplier,
repository, and pinned version is recorded, giving a starting point for a manufacturer's own
dependency vulnerability scanning.

## 3. Secure design and implementation

`#![forbid(unsafe_code)]` on every governed crate is a compiler-enforced memory-safety guarantee
covering the large majority of the codebase (`crates/`), not just a coding-standard recommendation —
see [ADR-005](../../../docs/adr/ADR-005-pure-rust-project-boundary-and-dependency-policy.md). The
byte-verified evidence pattern
([ADR-007](../../../docs/adr/ADR-007-compliance-evidence-and-generated-artifact-ownership.md)) —
every baked asset's `report.json` records a SHA-256 digest that CI's `verify` step re-derives and
compares — is a build-integrity control: a tampered or accidentally-corrupted generated artifact
fails CI rather than shipping.

## 4. Security verification

`cargo test --locked` and the baker `verify` subcommands run on every push
(`.github/workflows/ci.yml`); no dedicated fuzzing, dependency-vulnerability scanning (e.g.
`cargo audit`), or penetration testing is currently part of this project's own CI — a gap a
manufacturer should close in their own security verification plan rather than assume is covered.

## 5. Security update management

Not applicable in the current architecture: MduX-rust has no runtime network connectivity, no update
mechanism, and no fielded-device communication path. A manufacturer who adds any of these on top of
MduX-rust owns the entire security-update-management activity group themselves; nothing in this
project provides scaffolding for it today.

## 6. Security guidelines for users

Not applicable for the same reason as §5 — MduX-rust ships no operator-facing security guidance
because it has no network-facing or credential-handling surface to guide users on.

## Justification records

```json
{
  "justification_id": "JUS-005",
  "standard": "IEC 81001-5-1",
  "clause_ref": "IEC 81001-5-1:2021 §... [ verify against current edition — see docs/iec81001/README.md's numbering caveat ]",
  "rationale": "The governed/adapter/tools trust-zone split confines unsafe code and native SDK bindings to a single, narrow adapter crate, and #![forbid(unsafe_code)] on every governed crate makes the rest of the codebase memory-safe by construction rather than by review discipline alone.",
  "evidence_refs": [
    "docs/adr/ADR-005-pure-rust-project-boundary-and-dependency-policy.md",
    "docs/adr/ADR-012-presentation-adapter-crates-and-shader-artifacts.md",
    "docs/governance/soup-register.toml"
  ]
}
```
