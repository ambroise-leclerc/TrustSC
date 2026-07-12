# ADR-019: Regulatory standards reference corpus and software development file scaffold

## Status

Accepted

## Context

`README.md`'s "Feuille de route" and `docs/regulatory-compliance.md`'s roadmap section both commit to
two pieces of scaffolding this project had not yet delivered: an LLM-consumable corpus of regulatory
standard references (IEC 62304, ISO 13485, ISO 14971, IEC 62366-1, IEC 81001-5-1), and a
`software_development_file/` tree with `templates/` (blank, for any manufacturer) and `regulatory/`
(filled in for MduX-rust itself) subtrees.

The sister C++ project (`MduX`) already prototyped a similar system for two of these standards. That
prototype surfaced a reusable *pattern* — modular files grouped by clause range, a compact per-standard
index, JSON automation schemas with a consistent id/`$ref` convention — but also concrete problems this
project's version needed to avoid:

- Its "AI Reference" documents were near-verbatim paraphrases of the actual IEC/ISO standard text.
  IEC/ISO standards are commercial, copyrighted documents, not public domain — reproducing large
  portions of them on a public repository is a real legal exposure, not a hypothetical one.
- It carried three overlapping tiers per standard (modular files, a monolithic "AI Reference" doc, and
  a project-specific "Framework" doc) that had visibly drifted apart from each other over time.
- Its top-level `docs/` had no index tying the tree together, and its own `CLAUDE.md` never referenced
  the regulatory docs at all — so an AI coding agent would not discover them unprompted even though
  they existed.
- Its IEC 62304 clause numbering did not match the standard's actual structure (flat top-level
  "clauses" 5-16 where the real standard has one clause 5 with subclauses 5.1-5.8), which undermines
  the entire point of a citable reference.
- Its schema index documented roughly three times as many schemas as were actually implemented.

## Decision

- Each standard gets exactly two documentation tiers under `docs/<standard>/`: modular files grouped
  by real clause range (`01-*.md` through `0N-*.md`), and one compact `AI-Reference.md` index over
  those modules (clause + title + one-sentence pointer, not parallel full content). No third
  project-specific "Framework" tier — `docs/regulatory-compliance.md`, this ADR trail, and
  `software_development_file/regulatory/` already are the "applied to this project" layer.
- No clause of any standard is ever quoted or closely paraphrased. Every module section is original
  explanatory prose written against the clause's number and title (structural facts, not copyrightable
  prose), pointing at a real MduX-rust mechanism — a type, another ADR, a CI step, an example — wherever
  one genuinely applies. `docs/governance/citation-convention.md` defines the shared citation-key
  format (`"<Standard> §<clause> <title>"`) and the `Justification` object
  (`docs/iec62304/schemas/justification.schema.json`, shared by all five standards) that ties a
  specific design decision to a specific clause with a rationale and evidence pointers — this is the
  concrete mechanism for citing "the original paragraph" without reproducing it.
- Per-standard JSON schemas (`docs/<standard>/schemas/*.schema.json`) for `Requirement`, `Hazard`, and
  `VerificationCase` are field-aligned with `crates/mdux-governance/src/lib.rs`'s existing Rust types,
  so a future `serde`-based JSON export from `ComplianceProgram` (not built in this change) can match
  these schemas without a redesign. `mdux_core::SafetyClass`'s Class-B/Class-C-only scope is carried
  into every schema that touches classification — no schema implies Class A support.
- `docs/README.md`, `docs/regulatory-compliance.md`, root `README.md`, and this repo's own `CLAUDE.md`
  are updated to point at the new corpus and at `software_development_file/`, closing the discovery gap
  found in the C++ project's version.
- `software_development_file/templates/` holds blank, standard-by-standard fill-in-the-blank documents
  any manufacturer can start from; `software_development_file/regulatory/` holds the same tree filled
  in for MduX-rust itself, citing real ADRs, crate types, and examples rather than placeholder text.
- No changes to `mdux-governance`/`mdux-core` Rust code are made as part of this ADR — this is a
  documentation and JSON-schema change only.

## Consequences

### Positive
- A developer or AI coding agent working on regulatory/compliance content has a single, discoverable,
  citable corpus per standard, with a consistent format across all five.
- The `Justification` object and schema field-alignment give a concrete, checkable mechanism for tying
  a design decision to a specific clause, satisfying the roadmap's "justifications with references to
  the original paragraphs" goal without a copyright problem.
- The corpus is structured so a later `ComplianceProgram` JSON export slots into the existing schemas
  rather than requiring them to be rewritten.

### Negative / limitations
- The schemas are not yet wired to a real `ComplianceProgram` export — `trace_matrix_export()` and
  `audit_export()` still return pipe-delimited text (`crates/mdux-governance/src/lib.rs`), so today the
  schemas describe an intended future shape, not a live data contract. Building that export is left as
  future work.
- IEC 81001-5-1's exact clause numbering is less certain than the other four standards (it is a newer
  standard, structured as a crosswalk onto IEC 62443-4-1 activities); `docs/iec81001/README.md` and
  `docs/iec81001/AI-Reference.md` carry an explicit caveat that its clause references are a
  navigational aid to be checked against the current edition before use in a real submission.
- This corpus is a navigational and explanatory aid, not a substitute for the manufacturer's own
  licensed copy of each standard or their own regulatory/quality expertise — the same disclaimer
  `docs/regulatory-compliance.md` already states about the rest of this project's compliance scaffolding
  applies here too.
