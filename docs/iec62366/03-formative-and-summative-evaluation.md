# IEC 62366-1: Formative and summative evaluation

## Module overview

The last two sub-clauses of §5 close the usability engineering process's loop: formative
evaluation feeds back into design iteratively as the interface is built, and summative evaluation
confirms — usually through observed use by representative users — that the finished interface,
in particular its hazard-related use scenarios, is acceptably safe. This is the module where this
corpus draws its sharpest line between what MduX-rust genuinely automates and what it cannot touch
at all: `--verify-ui` (ADR-016) is a real, running piece of formative-evaluation-adjacent
machinery, and summative evaluation is, honestly, entirely outside a UI SDK's reach.

**Key areas covered:**
- Formative evaluation and what `--verify-ui` does and does not substitute for
- Summative evaluation and why MduX-rust provides no mechanism for it
- How both connect back to the usability engineering file (module 01, §4.2)

---

## §5.6 Formative evaluation

Formative evaluation is analysis and/or testing conducted *during* design and development to
generate or refine user interface requirements and design, and to identify use errors and their
causes before the interface is finalized — inspection-based (expert review, heuristic analysis)
and test-based (usability tests with representative users on prototypes) methods are both in
scope, iterated as many times as the design changes.

`--verify-ui` (ADR-016, `docs/adr/ADR-016-automated-ui-verification-and-manual-generation.md`)
is the framework mechanism closest to this clause, and it is worth describing precisely so its
relevance is neither oversold nor dismissed. It renders each compiled screen offscreen, through
the *same* command-recording and resource-building code path used for a real presented frame —
deliberately, so there is no separate "test renderer" that could drift from what ships — and runs
a fixed check suite against the captured pixels:

- **`GoldenBounds`** — every safety-critical or positioned node's rendered ink stays inside its
  specified bounds.
- **`ChromeColor`** — sampled pixels in a node's glyph-free regions match the theme token's exact
  expected byte value.
- **`TextPresence`** — ink coverage inside a text node's bounds falls within a band consistent
  with the active locale's compiled glyph run, catching blank, clipped, or garbled text.
- **`InkContainment`** — no node's rendered ink appears outside its own bounds, checked per node
  per locale; because compiled bounds are statically disjoint (ADR-014), this simultaneously
  proves no rendered overflow and no rendered overlap in every supported translation.
- **`ColorHash`** — an exact, backend-scoped SHA-256 of a golden region's rendered bytes, pinned
  against committed lavapipe baselines in CI as byte-exact regression evidence.

Run with `--locales=all`, this catches a real and common class of formative-evaluation finding —
a translation that overflows its allocated box, a status color that silently drifted from the
theme table, a label that renders blank because a text key resolved to the wrong run — for every
screen, every locale, on every CI run, which is more thorough and more consistent than a human
reviewer re-checking every locale by hand on every change. Scenario scripts
(`examples/<app>/verify/scenarios/*.toml`, ADR-016 §4) extend this to simple interaction
sequences: inject a `WidgetEvent`, assert the expected `FrameInputs` echo, capture and check an
offscreen frame — a scripted, repeatable stand-in for "does the interface end up in the right
state after this sequence of operator actions."

What `--verify-ui` is **not**, and this corpus states plainly rather than implying otherwise: it
is not a usability test. Every check is a property of rendered pixels against a specification the
same compiler produced — it can prove an interface renders what was specified, and it can prove a
scripted interaction sequence produces the expected state, but it cannot reveal that the
specification itself is confusing, that a real clinician under time pressure presses the wrong
button because two controls look too similar, or that alarm phrasing is misread. Those are exactly
the findings formative evaluation with representative users exists to surface, and no amount of
rendered-truth checking substitutes for observing an actual user. `--verify-ui`'s evidence
belongs in the usability engineering file (§4.2) as inspection-based formative evaluation evidence
for the narrow properties it checks — legibility-by-fit, color correctness, layout containment —
alongside, never instead of, a manufacturer's own test-based formative evaluation with real users.

## §5.7 Summative evaluation

Summative evaluation confirms that the finished user interface is safe and effective for its
intended users, uses, and environments — for hazard-related use scenarios in particular, this
typically means observing representative users perform representative tasks under realistic
conditions and analyzing the use errors (if any) that occur, with a documented rationale for the
user sample, task selection, and pass/fail criteria.

MduX-rust provides no mechanism for this clause, and none of its existing tooling is a
partial substitute the way `--verify-ui` is for §5.6. There is no human-subject testing
infrastructure, no protocol for recruiting or characterizing a representative user sample, no
task-performance measurement, and no statistical methodology for evaluating results — all of
that is inherently outside what any UI SDK can automate, because it requires actual humans
operating the actual device (or a sufficiently faithful simulation of it) in conditions
representative of real use. Stating this plainly matters more here than anywhere else in this
corpus: overclaiming automated summative evidence would be actively dangerous for a Class C
device's usability file, precisely the kind of overclaiming `docs/regulatory-compliance.md`'s
wording discipline ("supports," "provides evidence for" — never "ensures," "is compliant with")
exists to prevent.

What MduX-rust's governance types can hold, once a manufacturer has actually conducted a
summative evaluation, is the *record* of its outcome: a `usability-engineering-record.schema.json`
entry with `evaluation_type: "Summative"`, the `use_errors_identified` the study surfaced, and
the `risk_control_measures` (real `RequirementId`s) that followed from it, cross-referenced into
`mdux_governance::VerificationCase` the same way any other verification evidence is
(`method: "demonstration"`, `evidence`: a pointer to the study report) — see
`docs/iec62304/schemas/verification-case.schema.json` and
`docs/iec62304/04-development-implementation-and-testing.md` for the general pattern this
reuses. This is recordkeeping for evidence generated entirely outside MduX-rust, not evidence
generation.

---

## Related documents

- [Scope and general requirements](01-scope-and-general-requirements.md)
- [Use specification and user interface design](02-use-specification-and-ui-design.md)
- [ADR-016: Automated UI verification and manual generation](../adr/ADR-016-automated-ui-verification-and-manual-generation.md)
- [AI-Reference index](AI-Reference.md)
- [Citation convention](../governance/citation-convention.md)
