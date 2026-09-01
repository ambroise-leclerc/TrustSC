---
name: regulatory-citations
description: Cite regulatory standards (IEC 62304, ISO 13485, ISO 14971, IEC 62366-1, IEC 81001-5-1) correctly in this repo. Use when writing or reviewing an ADR, a software_development_file/ document, code comments, or any text that claims alignment with a standard clause — covers the citation-key format, the Justification JSON object, and the no-reproduction rule.
---

# Regulatory citations

The authoritative convention is `docs/governance/citation-convention.md`; this skill is the
operational recipe. The linter (`cargo run --locked -q -p trustsc-docs-lint -- check`) enforces it in
CI — run it after any edit under `docs/` or `software_development_file/`.

## Navigation recipe

1. Identify the standard: software lifecycle → `docs/iec62304/`, QMS → `docs/iso13485/`,
   risk management → `docs/iso14971/`, usability → `docs/iec62366/`, cybersecurity →
   `docs/iec81001/`.
2. Open that folder's `README.md` — its Modules table maps clause ranges to `NN-*.md` files.
3. Use the folder's `AI-Reference.md` for fast one-row-per-clause lookup, then open the linked
   module for the explanatory prose and TrustSC cross-references.

## Citation key format

One canonical string per clause, reused verbatim everywhere (module heading, AI-Reference row,
`clause_ref` field):

```
<Standard> §<clause>[.<subclause>] <Short clause title>
```

Example: `IEC 62304:2006 §5.2 Software development planning`

Only these five standard identifiers are valid (edition year included, exactly):
`IEC 62304:2006`, `ISO 13485:2016`, `ISO 14971:2019`, `IEC 62366-1:2015`, `IEC 81001-5-1:2021`.

Caveats:
- IEC 62304 amendment content (AMD1:2015) is noted in prose, never in the key.
- IEC 81001-5-1 clause numbering in this corpus is provisional — read the caveat at the top of
  `docs/iec81001/README.md` before citing it.

## The Justification object

When a design decision needs a formal link to a clause, emit a fenced `json` block conforming to
`docs/iec62304/schemas/justification.schema.json` (one schema shared by all five standards):

```json
{
  "justification_id": "JUS-NNN",
  "standard": "IEC 62304",
  "clause_ref": "IEC 62304:2006 §5.3.3 Identify segregation necessary for risk control",
  "rationale": "Original prose: why/how TrustSC's design addresses this clause.",
  "requirement_id": "REQ-... (optional, a real trustsc-governance RequirementId)",
  "evidence_refs": ["docs/adr/ADR-005-....md", "crates/trustsc-core/src/lib.rs"]
}
```

- `justification_id` is `JUS-NNN`, sequential and unique across the **whole corpus** — grep
  `docs/` and `software_development_file/` for the highest existing `JUS-` number before
  allocating a new one.
- `standard` is one of: `IEC 62304`, `ISO 13485`, `ISO 14971`, `IEC 62366-1`, `IEC 81001-5-1`
  (no edition year here), and must match the `clause_ref` prefix.
- `evidence_refs` entries are real repo paths (files, ADRs, example directories) — the linter
  checks they exist.

Worked examples: `software_development_file/regulatory/IEC_62304/SAD.md`.

## Hard rules

- **Never reproduce or closely paraphrase a standard's normative text.** IEC/ISO standards are
  copyrighted commercial documents. The corpus is original explanatory prose written against
  clause numbers/titles only. If you need the normative wording, cite the clause and tell the
  reader to consult their licensed copy.
- Verify clause numbers against the standard's `AI-Reference.md`, never assume them from memory
  or other sources.
- Point rationales at concrete TrustSC mechanisms (a governance type, an ADR, a validation in
  `ComplianceProgram::validate()`), not at generic compliance language.
