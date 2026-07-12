# IEC 62366-1: Use specification and user interface design

## Module overview

The first five sub-clauses of §5 (Usability engineering process): defining what the device is
for and who uses it, finding the use scenarios where a use error could cause harm, turning that
analysis into a concrete user interface specification, planning how the interface will be
evaluated, and finally designing and implementing it. This is the module where TrustSC's real
mechanisms — the MedUI DSL, its compile-time text-budget and layout checks, and its
requirement-bound safety-critical annotation — have the most to say, because §5.3 and §5.5 are
about the interface as a designed, specified artifact, which is exactly what the DSL produces.
§5.1, §5.2, and §5.4 are analysis and planning activities upstream of any tooling, and this
module says so honestly rather than stretching a code mechanism to cover ground it doesn't.

**Key areas covered:**
- Use specification: intended use, user population, use environment
- Identifying frequently used functions and hazard-related use scenarios
- The user interface specification
- The user interface evaluation plan
- User interface design and implementation

---

## §5.1 Use specification

The use specification states the device's intended medical indication, intended patient
population, intended part/system of the body, intended user profile(s), intended use
environment(s), and — where relevant — the operating principle, in enough concrete detail that
frequently used functions and foreseeable use scenarios can be identified against it in §5.2. It
is entirely an input the manufacturer supplies about their own device; nothing about a UI SDK can
generate or infer a clinical intended-use statement.

TrustSC's relevant contribution is indirect: the `trustsc_core::DeviceContext` type
(`crates/trustsc-core/src/lib.rs`) that every `Framework` is built around carries a device identity
and `SafetyClass`, and the example applications (`examples/hello_world`, `examples/class_c_monitor`)
show the pattern of stating a device's identity and classification alongside its compliance
program. That is a place to *anchor* a use specification's device identity to the same
`DeviceContext` a `ComplianceProgram` validates against — it is not a use-specification authoring
tool, and this corpus does not claim it produces or validates the content of a use specification
in the sense §5.1 requires (intended user profile, use environment, operating principle are all
outside `DeviceContext`'s fields entirely).

`examples/class_c_monitor`'s NeuroSense 500 depth-of-anesthesia monitor is the corpus's one
worked illustration throughout this module and the next: its use specification (an intended user
of a trained anesthesia clinician, an intended environment of an operating room, and an intended
function of continuous sedation-index monitoring with an alert on a hazardous trend) is assumed
context for the example, not something derived from its code — the code exists *because of* a
use specification like that one, in the same direction §5.1 → §5.2 → §5.3 flows.

## §5.2 Identify frequently used functions and hazard-related use scenarios

Against the use specification, the manufacturer identifies which functions are used frequently
enough that habituation and automaticity become a design concern, and works through foreseeable
use scenarios to find the ones where a use error could contribute to a hazardous situation —
hazard-related use scenarios in this standard's sense (§3 above). This is a structured brainstorm
against real clinical workflow, not a mechanical derivation from a UI's structure; TrustSC
cannot run it, and nothing in the framework claims otherwise.

Where TrustSC connects to this clause is at the output side, once the manufacturer has
identified a hazard-related use scenario: `trustsc_governance::Hazard` (`id`, `description`,
`controlled_by`) is a structured place to record that a use scenario contributes to a
hazard, and `Hazard::validate()`'s requirement that `controlled_by` be non-empty enforces the same
discipline IEC 62304 §7.2.1 applies to a software-contributed hazard (`docs/iec62304/06-risk-management-process.md
§7.2 Risk control measures`) — a hazard identified through use-scenario analysis must trace to at
least one requirement implementing its control, not remain a paper observation. The
`usability-engineering-record.schema.json` in this folder's `schemas/` directory gives the
scenario itself (not just the resulting hazard) a recordable shape: `use_scenario.description`,
`use_scenario.hazard_related`, and `use_scenario.frequency_of_use` are exactly the §5.2 outputs —
a frequently-used function's scenario and a hazard-related scenario are both representable, and
distinguishable by the `hazard_related` flag.

A concrete illustration in the worked example: a clinician failing to notice a sedation-index
alert in time is a hazard-related use scenario for the NeuroSense 500. `examples/class_c_monitor`'s
alert path — driven by `Classifier1D::predict()`'s deterministic classification and rendered
through a `StatusIndicator` node — exists as the risk control measure for that scenario, and
`docs/iec62304/06-risk-management-process.md §7.2.1` already documents it from the IEC 62304 side.
This standard's contribution is the analysis step that would justify *why* that alert path's
timing, visibility, and phrasing were chosen the way they were — an analysis this corpus records
the conclusion of (via the schema in this folder) but does not perform.

## §5.3 User interface specification

The user interface specification translates the use specification and the hazard-related use
scenarios into concrete requirements for the interface itself: what information must be
displayed, what controls must exist, how they must be labeled, and — critically for a
hazard-related scenario — what interface properties are the actual risk control (a specific
alert's color, position, or persistence, not just "an alert exists"). This is the clause where
TrustSC's MedUI DSL is most directly relevant, because a `.medui` file *is* a formal,
machine-checked expression of a big part of a user interface specification, for the narrow slice
of "structure, layout, text content, and safety-critical bindings" — not for the broader
clinical-workflow specification §5.3 as a whole calls for.

Concretely, `docs/dsl/component-dictionary.md`'s component set gives a manufacturer typed,
compile-time-enforced vocabulary for exactly the kind of requirement a UI specification states:
a `StatusIndicator` binds an enumerated device-state display to a `requirement` id and can be
marked `@safety_critical`, so "the sedation-state indicator exists, is bound to requirement
REQ-X, and its bounds/color are pinned as golden evidence" is a specification statement the
compiler enforces rather than a sentence in a document nobody re-checks. A `CriticalButton`
similarly requires a `requirement` id and a bounded `SystemEvent` — ADR-011 (`docs/adr/ADR-011-medui-safety-monitor-and-vulkan-viewport-contract.md`)
describes this as keeping "UI traceability... compatible with the current governance model,"
which is precisely a user-interface-specification-to-risk-control-measure link, expressed in code
instead of prose.

The DSL's i18n/text-budget policy (ADR-010, `docs/adr/ADR-010-medui-i18n-and-text-budget-policy.md`)
is a second, distinct piece of §5.3: a specification that says "this label must be legible in
every approved language" is only meaningful if it is actually checked, and `build.rs`'s
compile-time measurement of every approved locale's rendered width against a node's allocated
bounds — rejecting the screen outright if the widest translation doesn't fit — turns that
specification statement into a build gate rather than a hope. `VulkanViewport` and `SignalTrace`
(component dictionary, ADR-011/ADR-018) are the framework's answer to a different §5.3 concern:
where the specification calls for a reserved region for direct imaging or a raw physiological
waveform, these primitives compile to a bounded, reserved region descriptor rather than letting
arbitrary render logic leak into the UI layer — a specification constraint enforced structurally.

What §5.3 asks for that the DSL does not and cannot provide: the clinical rationale for *why*
a given color, wording, or layout is the right choice for the intended user population — that
judgment (informed by §5.2's analysis and, ultimately, formative/summative evaluation) precedes
authoring the `.medui` file and is not recoverable from it.

## §5.4 User interface evaluation plan

The evaluation plan states which use scenarios will be evaluated, by what method (formative,
summative, or both), against what acceptance criteria, and — for hazard-related scenarios
selected for summative evaluation — why they were selected. This is a planning document the
manufacturer writes before evaluation begins; TrustSC has no artifact that plays this role.

The closest the framework comes is structural rather than substantive: `--verify-ui`'s check
vocabulary (`GoldenBounds`, `ChromeColor`, `TextPresence`, `InkContainment`, `ColorHash` — ADR-016,
`docs/adr/ADR-016-automated-ui-verification-and-manual-generation.md`) is itself a fixed,
documented "plan" for one specific kind of check (rendered-truth verification against a compiled
specification), run automatically for every screen and every approved locale. It is not a
substitute for a §5.4 evaluation plan spanning formative and summative evaluation with real users:
it says nothing about which use scenarios matter, what acceptance criteria apply to human
performance, or how evaluators are recruited — all central §5.4 content. A manufacturer's actual
evaluation plan should reference `--verify-ui`'s reports as one evidence source among several, not
treat them as the plan itself.

## §5.5 User interface design and implementation

This is where the interface specified in §5.3 is actually built, and it is the clause module 03's
formative evaluation discussion (§5.6) sits closest to — design and formative evaluation are
meant to iterate together, not run as a single pass. TrustSC's widget architecture (ADR-015,
`docs/adr/ADR-015-widget-organization-principles.md`) is the framework's implementation substrate
for this clause: a closed, compiler-enforced widget set (`CriticalButton`, `Button`, `TextInput`,
`Label`, `Clock`, `NumericDisplay`, `StatusIndicator`, `Panel`, `Image`, `VulkanViewport`,
`SignalTrace`) where structure is retained only at compile time and all interaction state lives in
the application, chosen specifically (per ADR-015's five-framework survey) because it gives every
implemented screen a static structure that can be pinned as evidence — directly useful once §5.6/
§5.7 evaluation needs something concrete to evaluate against.

Several implementation-level design choices in TrustSC exist specifically to reduce
use-error-prone interface implementation, worth citing here even though they were motivated by
determinism/certifiability concerns first and usability second:

- **`Button` vs `CriticalButton`** (component dictionary, ADR-015) — the DSL forces a declaration
  choice between an application-semantic press and a framework-governed system action
  (`on_press` on `Button` is a compile error), which prevents an implementation mistake where a
  safety-relevant action is wired through the generic, unaudited path.
- **`TextInput`'s bounded charset and controlled-component discipline** (ADR-015) — content is
  restricted to a baked, approved character set and a declared maximum length, checked both at
  compile time (budget) and at the `set_text` runtime boundary (charset/length), which forecloses
  a whole class of "operator enters something the display can't render correctly" use errors by
  construction rather than by review.
- **Precise positioning's containment and no-overlap checks** (ADR-014, component dictionary
  "Precise positioning") — every positioned component is verified at build time to stay inside
  its container and not overlap any other node, preventing an implementation-time layout mistake
  that could obscure a control or a status indicator.

None of this reaches the parts of §5.5 that are genuinely about human factors — control spacing
appropriate to gloved hands, alarm phrasing tested for comprehension, color choices validated
against the intended clinical lighting environment — those remain the manufacturer's design
judgment, informed by §5.1/§5.2 and validated by §5.6/§5.7, applied *through* the DSL's
vocabulary rather than supplied by it.

---

## Related documents

- [Scope and general requirements](01-scope-and-general-requirements.md)
- [Formative and summative evaluation](03-formative-and-summative-evaluation.md)
- [MedUI DSL overview](../dsl/overview.md)
- [MedUI DSL component dictionary](../dsl/component-dictionary.md)
- [AI-Reference index](AI-Reference.md)
- [Citation convention](../governance/citation-convention.md)
