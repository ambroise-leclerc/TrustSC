---
name: sdf-documents
description: Fill in or review software_development_file/ documents (SAD, SDD, SOUP list, Usability Engineering File, Cybersecurity SAD, Risk Management File). Use when editing the templates/ or regulatory/ trees, or when a manufacturer-facing compliance document needs to cite the regulatory corpus.
---

# Software development file documents

`software_development_file/` holds two mirrored trees (ADR-019, see its `README.md`):

- `templates/` — blank documents any manufacturer building on TrustSC fills in for their own
  device. Keep them product-neutral: structural headings, guidance prose, placeholder tables —
  no TrustSC-specific claims.
- `regulatory/` — the **same** documents filled in for TrustSC itself. This is the worked
  example; keep the two trees structurally in sync (a heading added to one is added to the other).

Documents per standard: `IEC_62304/{SAD,SDD,SOUP}.md`, `IEC_62366/Usability_Engineering_File.md`,
`IEC_81001/Cybersecurity_SAD.md`, `ISO_13485/README.md`, `ISO_14971/Risk_Management_File.md`.

## The section citation pattern

Each section that addresses a clause carries a blockquote citation header (the exact citation
key), prose, and — where a design decision needs a formal link — a fenced `Justification` JSON
block. Follow the worked examples in `software_development_file/regulatory/IEC_62304/SAD.md`:

````markdown
> `IEC 62304:2006 §5.3.3 Identify segregation necessary for risk control`

Prose explaining how TrustSC addresses this clause, citing concrete mechanisms...

```json
{ "justification_id": "JUS-00N", "standard": "IEC 62304", "clause_ref": "...", "rationale": "...", "evidence_refs": ["..."] }
```
````

For the citation-key grammar, `Justification` field rules, and JUS-id allocation, use the
`regulatory-citations` skill / `docs/governance/citation-convention.md`. The linter
(`cargo run --locked -q -p trustsc-docs-lint -- check`) validates both after your edits.

## Hard rules

- **Summarize, never duplicate, machine registers.** The filled `SOUP.md` points at and
  summarizes `docs/governance/soup-register.toml`; it must not re-list entries that would drift.
  Same principle for anything CI already verifies (evidence reports, trace matrices).
- Never reproduce standards' normative text — original prose only (see `regulatory-citations`).
- Be honest about gaps: where TrustSC deliberately does *not* provide something (a QMS, a
  clinical evaluation, trend analysis), the filled documents say so explicitly and assign it to
  the manufacturer — follow the existing tone in `regulatory/` and
  `docs/regulatory-compliance.md`.
- Evidence pointers cite real repo paths (ADRs, crates, examples, CI steps) so an auditor can
  follow every claim to its mechanism.
