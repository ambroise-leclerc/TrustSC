*This is a compact, complete index over `docs/iec62366/`'s modular files — one row per clause, not a
parallel transcription. It contains no reproduced standard text (see
[`../governance/citation-convention.md`](../governance/citation-convention.md)); use it to find which
module covers a given clause, then open that module for the actual explanatory prose and TrustSC
cross-references.*

# IEC 62366-1:2015+AMD1:2020 — AI-Reference index

Every clause referenced by `docs/iec62366/01-*.md` through `03-*.md` is listed below, in clause
order, with a one-sentence pointer and a link to its detail. No clause is stubbed or condensed to a
placeholder — if a row exists here, its module has real content.

## §1-§4 — Scope and general requirements ([detail](01-scope-and-general-requirements.md))

- **§1 Scope** — applies to use-related safety of a device's user interface, not just correct
  function; TrustSC narrows a small, structural slice of this (text/layout/safety-critical
  binding) and says so plainly. [→](01-scope-and-general-requirements.md#1-scope)
- **§2 Normative references** — ties to ISO 14971 (risk management) and is referenced by IEC 62304
  §2 for devices with a user interface. [→](01-scope-and-general-requirements.md#2-normative-references)
- **§3 Terms and definitions** — user interface, use error vs. abnormal use, hazard-related use
  scenario, usability engineering file, formative/summative evaluation. [→](01-scope-and-general-requirements.md#3-terms-and-definitions)
- **§4.1 Application of this standard** — how the process applies to a device UI as a whole, and
  the user-interface-of-unknown-provenance (legacy UI) gap-analysis path, which TrustSC has no
  code path for. [→](01-scope-and-general-requirements.md#41-application-of-this-standard)
- **§4.2 Usability engineering file** — the manufacturer's traceable evidence collection;
  `usability-engineering-record.schema.json` gives one piece of it a recordable shape. [→](01-scope-and-general-requirements.md#42-usability-engineering-file)

## §5.1-§5.5 — Use specification and user interface design ([detail](02-use-specification-and-ui-design.md))

- **§5.1 Use specification** — intended use, user population, use environment; entirely a
  manufacturer input, anchored optionally to `DeviceContext`. [→](02-use-specification-and-ui-design.md#51-use-specification)
- **§5.2 Identify frequently used functions and hazard-related use scenarios** — a clinical-workflow
  analysis TrustSC cannot run; `Hazard.controlled_by` and the usability-engineering-record schema
  record its outputs. [→](02-use-specification-and-ui-design.md#52-identify-frequently-used-functions-and-hazard-related-use-scenarios)
- **§5.3 User interface specification** — the MedUI DSL (component dictionary, ADR-010/ADR-011) is
  a formal, compiler-enforced expression of the structure/text/safety-binding slice of this clause.
  [→](02-use-specification-and-ui-design.md#53-user-interface-specification)
- **§5.4 User interface evaluation plan** — a manufacturer-authored planning document; `--verify-ui`'s
  fixed check vocabulary is structurally adjacent but is not this plan. [→](02-use-specification-and-ui-design.md#54-user-interface-evaluation-plan)
- **§5.5 User interface design and implementation** — ADR-015's closed widget set, `Button` vs.
  `CriticalButton`, `TextInput`'s bounded charset, and precise-positioning containment checks all
  reduce implementation-time use-error surface. [→](02-use-specification-and-ui-design.md#55-user-interface-design-and-implementation)

## §5.6-§5.7 — Formative and summative evaluation ([detail](03-formative-and-summative-evaluation.md))

- **§5.6 Formative evaluation** — `--verify-ui`'s `GoldenBounds`/`ChromeColor`/`TextPresence`/
  `InkContainment`/`ColorHash` checks and scenario scripts are real, running inspection-based
  formative evidence for rendered-truth properties; not a substitute for testing with real users.
  [→](03-formative-and-summative-evaluation.md#56-formative-evaluation)
- **§5.7 Summative evaluation** — TrustSC provides no mechanism for this clause; the governance
  schema can only record the outcome of a study conducted entirely outside the framework. [→](03-formative-and-summative-evaluation.md#57-summative-evaluation)

---

For a design decision's justification against a specific clause here, use the `Justification` object
in [`../iec62304/schemas/justification.schema.json`](../iec62304/schemas/justification.schema.json),
citing the clause with its exact key from this index.
