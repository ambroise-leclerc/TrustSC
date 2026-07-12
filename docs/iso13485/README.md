# ISO 13485:2016 — Medical devices quality management systems

This folder is an LLM- and human-navigable breakdown of ISO 13485, the quality management system
(QMS) standard TrustSC's own scaffolding is designed to feed into, written as original explanatory
prose against the standard's real clause structure — see
[`../governance/citation-convention.md`](../governance/citation-convention.md) for why no normative
standard text is reproduced here, and for the citation-key format used throughout
(`ISO 13485:2016 §N.M Clause title`).

TrustSC is a software development kit, not an organization, and does not operate a QMS of its own
that a notified body would audit. §1's scope applies to the manufacturer who integrates TrustSC
into their device software and runs a QMS around that integration. This corpus is explicit,
sub-clause by sub-clause, about which mechanisms in TrustSC give evidence toward a manufacturer's
§4-§8 obligations, and which remain entirely the manufacturer's to build — see especially module 02
(§5-§6, almost entirely organizational governance with no software analogue).

## Modules

| Module | Clauses | Focus |
|---|---|---|
| [01-foundations-and-qms.md](01-foundations-and-qms.md) | §1-§4 | Scope, definitions, establishing the QMS, documentation requirements |
| [02-management-and-resources.md](02-management-and-resources.md) | §5-§6 | Management responsibility, resource management |
| [03-product-realisation.md](03-product-realisation.md) | §7 | Planning, customer-related processes, design and development (the deepest overlap with IEC 62304 §5), purchasing, production, monitoring/measuring equipment |
| [04-measurement-analysis-improvement.md](04-measurement-analysis-improvement.md) | §8 | Monitoring and measurement, nonconforming product, data analysis, CAPA |

## Other resources

- [`AI-Reference.md`](AI-Reference.md) — one-line-per-clause index across the whole standard.
- [`schemas/quality-management-system.schema.json`](schemas/quality-management-system.schema.json) —
  JSON Schema for a QMS-scope record (organization profile, device portfolio, applicable-requirements
  scope).
- [`../governance/citation-convention.md`](../governance/citation-convention.md) — citation format
  and the `Justification` object shared by all five standards in this corpus.
- [`../regulatory-compliance.md`](../regulatory-compliance.md) — how this corpus fits into the
  project's overall regulatory-compliance story and its stated scope limits.
- `software_development_file/regulatory/ISO_13485/README.md` (added in a later PR in this stack) —
  TrustSC's own filled-in scope note, which cites into this corpus.

## For AI agents

When generating or reviewing code, an ADR, or an SDF document that claims ISO 13485 alignment: find
the relevant module above by clause range, cite the clause using the exact citation-key format, and
be explicit about the boundary between what a mechanism in TrustSC gives evidence toward (mostly
§4.2's documentation requirements and §7.3's design-and-development control points, where the overlap
with IEC 62304 §5 is real) and what is purely organizational and has no software analogue at all
(most of §5-§6, most of §7.2/§7.4-§7.5, and the post-market obligations in §8.2) — every module in
this folder draws that line explicitly rather than stretching a type to cover a role it doesn't fill,
and is the pattern to follow.
