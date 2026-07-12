# Regulatory citation convention

This document defines how `docs/iec62304/`, `docs/iso13485/`, `docs/iso14971/`, `docs/iec62366/`,
`docs/iec81001/`, and `software_development_file/` cite regulatory-standard clauses, and how a
justification linking MduX-rust's design to a specific clause is structured. Every other file in the
regulatory corpus follows this convention rather than restating it.

This convention document is the first PR in a 9-PR stack that builds out the whole corpus (ADR-019);
`docs/iec62304/` lands in the next PR, and `docs/iso13485/`, `docs/iso14971/`, `docs/iec62366/`,
`docs/iec81001/`, and `software_development_file/` land in the PRs after that. Paths referenced below
resolve once the full stack is merged.

## No standard text is ever quoted here

IEC and ISO standards are commercial documents, not public domain — reproducing their normative text
is a copyright problem, and the C++ `MduX` project's attempt to do so (a "comprehensive markdown
version" of IEC 62304 "compiled from public source materials") is not repeated in this project.
Everything under `docs/<standard>/` is **original explanatory prose** written against the clause
*structure* (numbers and titles, which are not copyrightable in the way normative prose is), not a
transcription. A developer or auditor who needs the actual normative wording of a clause consults
their own licensed copy of the standard; this corpus tells them which clause to open and why it's
relevant to MduX-rust, and cites MduX-rust's own design (ADRs, crate types, examples) as far as
possible in its place.

## Citation key format

A single canonical string per clause, reused identically everywhere that clause is referenced:

```
<Standard> §<clause>[.<subclause>[.<sub-subclause>]] <Short clause title>
```

Examples:

- `IEC 62304:2006 §5.2 Software development planning`
- `IEC 62304:2006 §7.1 Analysis of software contributing to hazardous situations`
- `ISO 13485:2016 §7.3 Design and development`
- `ISO 14971:2019 §5.4 Identification of hazards and hazardous situations`
- `IEC 62366-1:2015 §5.6 Formative evaluation`
- `IEC 81001-5-1:2021 §... ` (see `docs/iec81001/README.md` — clause numbering for this standard is
  confirmed less precisely than the other four; treat citations there as provisional until checked
  against the current edition)

Rules:

- Standard identifier + edition year exactly as used elsewhere in this repo's `docs/regulatory-compliance.md`
  (`IEC 62304:2006`, `ISO 13485:2016`, `ISO 14971:2019`, `IEC 62366-1:2015`, `IEC 81001-5-1:2021`).
  IEC 62304:2006 was amended by AMD1:2015; where a clause's content depends on the amendment this is
  noted in prose, not in the citation key itself.
- The clause number matches the standard's real numbering (verified against the standard's own table
  of contents, not assumed from any other source) — this is what makes a citation checkable.
- The short title is the clause's own heading, kept short enough to read inline.
- The same string is used verbatim as: the modular file's `##`/`###` heading, the corresponding row in
  that standard's `AI-Reference.md`, and the `clause_ref` value in any `Requirement` or `Justification`
  object that cites it (`mdux-governance::Requirement.source_clause` is a free-text field already — this
  convention is simply what goes in it).

## The `Justification` object

`docs/iec62304/schemas/justification.schema.json` defines this shape once; it is shared by all five
standards (a justification's `standard` field says which one it's citing) rather than duplicated per
standard. This is the concrete mechanism for "a justification with a reference to the original
paragraph": a small, schema-validated record tying a design decision in MduX-rust to a specific clause,
with a human-readable rationale and pointers to the evidence (an ADR, a source file, an example, a
`mdux-governance` requirement) that substantiates it.

```json
{
  "justification_id": "JUS-001",
  "standard": "IEC 62304",
  "clause_ref": "IEC 62304:2006 §7.1 Analysis of software contributing to hazardous situations",
  "rationale": "mdux-governance::Hazard requires >=1 controlling Requirement, and ComplianceProgram::validate() rejects a Class C device with zero recorded hazards, giving §7.1's hazard analysis a machine-checked minimum rather than a paper-only record.",
  "requirement_id": "REQ-EEG-ALERT-LATENCY",
  "evidence_refs": [
    "crates/mdux-governance/src/lib.rs",
    "docs/adr/ADR-011-medui-safety-monitor-and-vulkan-viewport-contract.md",
    "examples/class_c_monitor"
  ]
}
```

Field notes:

- `justification_id` — `JUS-NNN`, sequential, unique across the whole corpus (not per-standard).
- `standard` — one of `IEC 62304`, `ISO 13485`, `ISO 14971`, `IEC 62366-1`, `IEC 81001-5-1`.
- `clause_ref` — a citation key in the format above.
- `rationale` — why/how MduX-rust's design addresses or informs this clause. Written in prose, not a
  restatement of the clause itself.
- `requirement_id` *(optional)* — cross-references a real `mdux-governance::RequirementId` when the
  justification backs a specific tracked requirement rather than a general design property.
  `crates/mdux-governance/src/lib.rs` defines this type; that crate has no `serde` support yet, so this
  is a documentation-level cross-reference today, not a live join — see `docs/adr/ADR-019-regulatory-standards-reference-corpus.md`
  for why wiring a real JSON export is left as future work rather than built here.
- `evidence_refs` — file paths, ADR filenames, or example directory names substantiating the rationale.
  Not a formal schema of its own; free-form strings a human or LLM can follow.

Justification objects are not collected into one giant registry file in this pass — they appear inline
(as fenced `json` blocks) in the `software_development_file/regulatory/` documents (added later in
this PR stack) where a specific design choice needs to cite its clause, and in `docs/<standard>/NN-*.md`
modules where a clause's explanatory prose points at a concrete piece of MduX-rust as its example. A future change could lift
these into a single validated array if the corpus grows large enough to need one; see
`docs/regulatory-compliance.md`.

## Related documents

- [`docs/iec62304/schemas/justification.schema.json`](../iec62304/schemas/justification.schema.json)
- [`docs/regulatory-compliance.md`](../regulatory-compliance.md)
- [`docs/governance/soup-register.toml`](soup-register.toml)
- [`docs/adr/ADR-019-regulatory-standards-reference-corpus.md`](../adr/ADR-019-regulatory-standards-reference-corpus.md)
