# ISO 14971: Risk analysis and evaluation

## Module overview

This module covers §5 (Risk analysis, in full: §5.1-§5.5) and §6 (Risk evaluation) — the front half
of the risk management process, where a device and its intended use are characterized, hazards and
the hazardous situations they can lead to are identified, the risk of each is estimated, and each
estimate is judged against acceptability criteria to decide whether risk control (module 03) is
needed. `docs/iec62304/06-risk-management-process.md` §7.1 already documents the software-specific
slice of hazard analysis (how software contributes to a hazardous situation once one has been
identified); this module covers the broader process that slice sits inside, and cites that module
rather than re-explaining its content.

**Key areas covered:**
- The risk analysis process and its required documentation
- Intended use and reasonably foreseeable misuse
- Identifying characteristics of the device related to safety
- Identifying hazards and the hazardous situations they can produce
- Estimating risk for each hazardous situation
- Evaluating whether an estimated risk requires further control

---

## §5.1 Risk analysis process

A manufacturer documents a risk analysis for a specific device, covering its intended use and
identifiable misuse, and updates it as the device or the understanding of its use evolves. MduX-rust
does not prescribe or automate a hazard-identification methodology (structured brainstorming, FMEA,
fault tree analysis, or otherwise) — that methodology choice, and its execution, are engineering
judgment calls made by the manufacturer's own risk analysis team. What MduX-rust provides is a place
to record the analysis's *outputs* once produced: `mdux_governance::Hazard` for the hazard itself, and
`docs/iso14971/schemas/risk-record.schema.json` for the more detailed per-hazardous-situation record
described in §5.4-§5.5 below.

## §5.2 Intended use and reasonably foreseeable misuse

Establishing who will use a device, in what clinical environment, for what purpose, and what a user
might plausibly do with it outside its intended use is a clinical and human-factors analysis —
squarely the domain of usability engineering (IEC 62366-1) rather than of a UI/ML rendering SDK.
MduX-rust provides no scaffolding for this subclause: it cannot infer a device's intended users or
foreseeable misuse from its own types, and nothing in `mdux-governance` or `mdux-ui` represents an
intended-use statement. `docs/iec62366/` is reserved for a future usability-engineering corpus module
covering this ground properly; it is not yet populated in this repository.

It is worth noting what MduX-rust *presupposes* here without performing the analysis itself: the
MedUI DSL's compile-time text-budget checking (ADR-010, `docs/dsl/`) requires a build to declare which
locales are approved and verifies every `t("key")` reference fits its allocated bounds in all of
them — but the decision of *which* locales a device's intended users actually need is an input to
that build step, supplied by the application, not a decision the compiler makes on the manufacturer's
behalf.

## §5.3 Identification of characteristics related to safety

Before hazards can be identified, a manufacturer characterizes the qualitative and quantitative
properties of the device that could bear on safety — how it interacts with its environment, what
performance and interoperability properties it depends on, and what limits its safe operation.
Several MduX-rust mechanisms produce exactly this kind of characteristic, even though none of them
constitute the analysis itself:

- `mdux_core::SafetyClass` and `DeterminismPolicy` (`crates/mdux-core/src/lib.rs`) characterize a
  software item's worst-case severity contribution and its runtime behavior (bounded frame time,
  whether runtime allocation or object creation is permitted) — properties directly relevant to
  whether a UI or inference component can miss a real-time deadline.
- `docs/governance/soup-register.toml` records, per third-party dependency, its `boundary_rationale`
  (which trust zone confines it) and `risk_controls` — a structured characterization of a specific
  kind of safety-relevant property (third-party dependency exposure) that grows fastest in a device's
  UI and ML layers, per `docs/regulatory-compliance.md`.
- ADR-017's strictly-ordered scalar arithmetic in `mdux-ml-runtime` (no SIMD, no FMA) is itself a
  documented safety-relevant characteristic of the ML inference software item: it is what makes
  host-computed golden vectors reproducible bit-for-bit on-device, and its absence would be a
  characteristic a risk analysis would need to flag.

## §5.4 Identification of hazards and hazardous situations

This is the clause `mdux_governance::Hazard` is built around. `Hazard { id, description,
controlled_by }` records a hazard's identity and description; `Hazard::validate()`
(`crates/mdux-governance/src/lib.rs`) rejects a hazard with an empty `controlled_by` list, so a hazard
cannot be recorded without at least one requirement that controls it — see
[`docs/iec62304/06-risk-management-process.md` §7.1](../iec62304/06-risk-management-process.md#71-analysis-of-software-contributing-to-hazardous-situations)
for how this same type and rule serve IEC 62304's software-contribution analysis; the two clauses
are read together rather than duplicated here.

`docs/iso14971/schemas/risk-record.schema.json` adds the layer ISO 14971 distinguishes but
`mdux_governance::Hazard` does not model on its own: a `hazard_ref` (cross-referencing a
`docs/iec62304/schemas/hazard.schema.json` id) paired with a free-text `hazardous_situation` field —
the specific circumstance in which that hazard's potential is actually realized. A single hazard can
give rise to more than one hazardous situation (the same delayed-update hazard could matter
differently in a bedside-monitoring situation versus a data-review situation), which is why the
schema keeps `hazard_ref` and `hazardous_situation` as separate fields on a per-risk-record basis
rather than folding hazardous-situation detail into `Hazard.description` itself.

`examples/class_c_monitor` (NeuroSense 500) is a concrete worked instance: a delayed or missed
sedation-index alert is the hazardous situation that motivates `Classifier1D::predict()`'s
deterministic, allocation-free inference path and `mdux-ml-runtime`'s fail-closed startup self-test
(ADR-017) as risk control measures — see module 03 §7.3 for the control side of this example.

## §5.5 Estimation of the risk(s) for each hazardous situation

For each identified hazardous situation, a manufacturer estimates the risk it presents — typically
expressed as a combination of the severity of harm that could result and the probability of that
harm occurring, using whatever qualitative or quantitative method is appropriate to the data
available. `docs/iso14971/schemas/risk-record.schema.json`'s `severity` and `probability` enum fields
(`Catastrophic`/`Critical`/`Serious`/`Minor`/`Negligible` and
`Frequent`/`Probable`/`Occasional`/`Remote`/`Improbable`) are the structured place a manufacturer
records the outcome of this estimation per risk record.

MduX-rust performs no part of the estimation itself — there is no failure-rate database, no
probability model, and no severity-scoring logic anywhere in the governed crates. The values in a
risk record reflect the manufacturer's own engineering and clinical judgment, informed by field data,
literature, or expert elicitation, exactly as the standard requires; the schema exists only so that
judgment has a consistent, machine-readable place to live once made.

---

## §6 Risk evaluation

Once a hazardous situation's risk has been estimated, it is compared against the acceptability
criteria established in the risk management plan (module 01 §4.4) to decide whether it is already
acceptable or whether risk control (module 03 §7) is required. `docs/iso14971/schemas/risk-record.schema.json`'s
`residual_risk_acceptable` boolean field is the structured place this judgment is recorded — the
schema is deliberately generic about *when* in the process that field is set: a manufacturer may
choose to instantiate one risk record per hazardous situation and update `residual_risk_acceptable`
in place as risk control measures are added, or to keep separate pre- and post-control records. Either
approach is a modeling choice left to the manufacturer's own risk management file structure, not
something this schema or `mdux-governance` prescribes.

`ComplianceProgram` itself encodes no risk-acceptability gate — it has no notion of "acceptable" or
"unacceptable" risk, only that a `Hazard` exists and is controlled by at least one `Requirement`
(module 01 §4.2 in the IEC 62304 corpus) and, for Class C, that at least one `Hazard` is recorded at
all. Whether a specific estimated risk clears the manufacturer's own acceptability bar is a judgment
this repository has no scaffolding for and does not attempt to encode; only the shape of the record
that captures the resulting decision is provided.

---

## Related documents

- [Scope and general requirements](01-scope-and-general-requirements.md)
- [Risk control and residual risk](03-risk-control-and-residual-risk.md)
- [IEC 62304 software risk management process](../iec62304/06-risk-management-process.md)
- [AI-Reference index](AI-Reference.md)
- [Citation convention](../governance/citation-convention.md)
