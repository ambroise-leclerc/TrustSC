# IEC 81001-5-1:2021 — Health software cybersecurity in product life cycle processes

> **Clause-numbering caveat.** IEC 81001-5-1 is a newer, less widely reproduced standard than the
> IEC 62304 / ISO 13485 / ISO 14971 / IEC 62366-1 material elsewhere in this corpus, and this
> module was written with lower confidence in its exact sub-clause numbering. Every citation key
> below uses a **broad section placeholder** (`§5`, `§6`, ...) rather than a fabricated precise
> sub-clause number, and is organized around the standard's known, real activity groups —
> SECURITY RISK MANAGEMENT, SECURE DESIGN, SECURE IMPLEMENTATION, SECURITY VERIFICATION, MANAGEMENT
> OF SECURITY-RELATED ISSUES / SECURITY UPDATE MANAGEMENT, and SECURITY GUIDELINES — rather than a
> specific clause tree. **Do not cite a clause key from this module in a real regulatory
> submission or `Justification` object without first checking it against your own licensed copy of
> the current edition of IEC 81001-5-1.** This caveat is repeated at the top of
> [`AI-Reference.md`](AI-Reference.md); treat both as provisional navigational aids, not as a
> verified clause map in the sense the other four standards in this corpus provide.

This folder is an LLM- and human-navigable breakdown of IEC 81001-5-1:2021, TrustSC's health-
software cybersecurity standard, written as original explanatory prose organized around the
standard's real activity groups — see
[`../governance/citation-convention.md`](../governance/citation-convention.md) for why no normative
standard text is reproduced here, and for the citation-key format used throughout (with the caveat
above applied to every key in this folder specifically).

IEC 81001-5-1 does not replace IEC 62304 — it layers security-specific activities onto a life
cycle IEC 62304 already governs, and is normatively linked to IEC 62443-4-1's secure-product-life-
cycle requirements. See [01-scope-and-relationship-to-iec62304.md](01-scope-and-relationship-to-iec62304.md)
for how the two standards' processes map onto each other.

## Modules

| Module | Approx. section | Focus |
|---|---|---|
| [01-scope-and-relationship-to-iec62304.md](01-scope-and-relationship-to-iec62304.md) | §4-§5 (approx.) | Scope, terms, relationship to IEC 62304's life cycle and to IEC 62443-4-1 |
| [02-security-risk-management.md](02-security-risk-management.md) | §5 (approx.) | Security risk management: threats and vulnerabilities alongside safety hazards |
| [03-secure-design-and-implementation.md](03-secure-design-and-implementation.md) | §6 (approx.) | Secure design principles and secure implementation practices |
| [04-security-verification-and-update-management.md](04-security-verification-and-update-management.md) | §6-§7 (approx.) | Security verification/testing, security update management, security guidelines for users |

## Other resources

- [`AI-Reference.md`](AI-Reference.md) — compact index across all four modules, with the clause-
  numbering caveat repeated at the top, for quickly locating which module covers a given activity
  group.
- [`schemas/`](schemas/) — a JSON Schema for a security risk record (`security-risk-record.schema.json`),
  field-aligned in spirit with `docs/iec62304/schemas/hazard.schema.json` but modeling security risk
  as a distinct concept from safety risk (see module 02). The shared `Justification` object lives at
  [`../iec62304/schemas/justification.schema.json`](../iec62304/schemas/justification.schema.json)
  and is not duplicated here.
- [`../governance/citation-convention.md`](../governance/citation-convention.md) — citation format
  and the `Justification` object shared by all five standards in this corpus; its own entry for
  `IEC 81001-5-1:2021` already flags this standard's citations as provisional.
- [`../regulatory-compliance.md`](../regulatory-compliance.md) — how this corpus fits into the
  project's overall regulatory-compliance story and its stated scope limits.
- `software_development_file/regulatory/IEC_81001/` (added in a later PR in this stack) —
  TrustSC's own filled-in cybersecurity SAD document, which cites into this corpus.

## For AI agents

When generating or reviewing code, an ADR, or an SDF document that claims IEC 81001-5-1 alignment:
find the relevant module above by activity group, cite using the citation-key format from
`../governance/citation-convention.md` but **flag the clause number as approximate** rather than
presenting it as verified, and prefer pointing at a real TrustSC mechanism (ADR-005's trust-zone
boundary, the SOUP register, ADR-007's byte-verified evidence pattern) over restating the standard's
requirement in the abstract — every module in this folder does this throughout and is the pattern to
follow. Where a module states that an activity group is not yet applicable (e.g. security update
management for a networked device, since TrustSC has no network stack today), preserve that
honesty rather than inventing a mechanism that does not exist in the repository.
