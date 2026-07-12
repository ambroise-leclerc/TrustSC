# ISO 13485:2016 quality management system — MduX-rust's scope note

> Filled-in example for MduX-rust itself. See
> [`software_development_file/templates/ISO_13485/README.md`](../../templates/ISO_13485/README.md)
> for the blank template, and [`docs/iso13485/README.md`](../../../docs/iso13485/README.md) for the
> underlying clause-by-clause guidance.

MduX-rust is a software development kit, not a manufacturer, and does not operate a quality
management system of its own that a notified body would audit. This document states, plainly, which
of this project's engineering artifacts a manufacturer's ISO 13485 QMS can draw on, and which parts
of §4-§8 remain entirely the manufacturer's own responsibility — echoing the standing disclaimer in
[`docs/regulatory-compliance.md`](../../../docs/regulatory-compliance.md).

## What MduX-rust's engineering artifacts can feed into your QMS

> `ISO 13485:2016 §4.2 Documentation requirements`, `§7.3 Design and development`

- **Design and development traceability** — `mdux_governance::ComplianceProgram::trace_rows()`/
  `trace_matrix_export()` (`crates/mdux-governance/src/lib.rs`) generate a requirement → verification
  → hazard matrix directly from typed data, mapping onto §7.3's design-input/output/verification/
  validation traceability expectations.
- **Document/record control for generated evidence** — every baked asset (fonts, shaders, ML model
  packages) ships a committed `package.json` + `report.json` pair with a SHA-256 digest, re-verified
  in CI ([ADR-007](../../../docs/adr/ADR-007-compliance-evidence-and-generated-artifact-ownership.md)) —
  a form of controlled record for those specific artifacts.
- **Design review record** — the ADR trail ([`docs/adr/README.md`](../../../docs/adr/README.md), 19
  ADRs at time of writing) is a dated record of design decisions and their rationale, usable as
  supporting evidence for a design review under §7.3.4.
- **Purchasing/supplier information for SOUP** — `docs/governance/soup-register.toml` records
  supplier, license, and support-model information per third-party dependency, relevant to §7.4's
  purchasing controls if a manufacturer treats MduX-rust's SOUP as purchased/acquired product.

## What your QMS must still supply

- Management responsibility, quality policy, and management review (§5) — MduX-rust has no concept
  of an organization's management structure.
- Human resources, infrastructure, and work-environment controls (§6).
- Customer-related processes and regulatory-submission-facing product realization steps (§7.2, most
  of §7.5-§7.6) beyond the design-and-development traceability noted above.
- Complaint handling, adverse-event/vigilance reporting, and CAPA (§8.2, §8.5) — MduX-rust's
  `ProblemReport` type records that a problem exists and whether it's closed, but implements none of
  the regulatory reporting obligations §8.2.3 describes.
- Everything about the manufacturer's own device beyond what's built with MduX-rust's UI/ML/governed
  types.

## Justification records

```json
{
  "justification_id": "JUS-006",
  "standard": "ISO 13485",
  "clause_ref": "ISO 13485:2016 §7.3 Design and development",
  "rationale": "ComplianceProgram::trace_rows() generates a requirement-to-verification-to-hazard matrix from typed data populated during development, giving a manufacturer's §7.3 traceability records a machine-derived source rather than a hand-maintained spreadsheet that can drift from the code.",
  "evidence_refs": [
    "crates/mdux-governance/src/lib.rs",
    "docs/regulatory-compliance.md"
  ]
}
```
