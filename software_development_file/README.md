# Software development file

This tree is the `software_development_file/` scaffold named in the root `README.md`'s roadmap and
`docs/regulatory-compliance.md`. It has two subtrees, mirroring each other standard-by-standard:

```
software_development_file/
├── templates/    # blank, fillable by any manufacturer building on TrustSC
└── regulatory/   # the same tree, filled in for TrustSC itself (added in the next PR in this stack)
```

Only `templates/` exists as of this PR — `regulatory/` is added in the PR immediately after it.

| Standard | Documents |
|---|---|
| IEC 62304 | `SAD.md` (architecture), `SDD.md` (detailed design), `SOUP.md` (SOUP list/justification) |
| IEC 62366 | `Usability_Engineering_File.md` |
| IEC 81001 | `Cybersecurity_SAD.md` |
| ISO 13485 | `README.md` (QMS scope note) |
| ISO 14971 | `Risk_Management_File.md` |

## `templates/`

Blank documents with section headers matching each standard's clauses, a citation blockquote per
section pointing at the relevant `docs/<standard>/` module, and `[ ... ]` placeholders. A
manufacturer building a device on TrustSC copies these into their own document set and fills them
in — they contain no TrustSC-specific content.

## `regulatory/`

*(Added in the PR immediately after this one — not present yet as of this PR.)*

The same documents, filled in for TrustSC itself: real architecture description, real SOUP
entries (derived from `docs/governance/soup-register.toml`), real citations into `docs/<standard>/`
and the ADR trail. These describe TrustSC as a software development kit — they are not, and do not
claim to be, a finished medical device's regulatory file. See `docs/regulatory-compliance.md`'s scope
disclaimer for what this project does and does not provide.

## How these connect to the rest of the corpus

Every document here cites into `docs/<standard>/` (the clause-by-clause explanatory corpus) using the
citation-key format defined in
[`docs/governance/citation-convention.md`](../docs/governance/citation-convention.md), and may embed
inline `Justification` objects
([`docs/iec62304/schemas/justification.schema.json`](../docs/iec62304/schemas/justification.schema.json))
tying a specific statement to a specific clause and its supporting evidence.
