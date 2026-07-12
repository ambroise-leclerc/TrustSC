# ISO 13485: Product realisation

## Module overview

§7 is where ISO 13485 is most relevant to TrustSC's actual engineering practice. It covers
planning how a product will be realized, determining and reviewing customer-related requirements,
design and development (§7.3 — the sub-clause with the deepest overlap with IEC 62304's software
development process, see `docs/iec62304/README.md`), purchasing, production and service provision,
and control of monitoring and measuring equipment. Where the standard's process shape and
TrustSC's own mechanisms genuinely line up — which is mostly in §7.3 — this module says so
concretely and cites the type or ADR involved; where §7's subject matter (physical production,
servicing, sterilization) has no software analogue, that is stated plainly.

**Key areas covered:**
- Planning of product realization
- Customer-related processes: determining, reviewing, and communicating requirements
- Design and development: planning, inputs, outputs, review, verification, validation, transfer,
  change control, and design files
- Purchasing and verification of purchased product
- Production and service provision
- Control of monitoring and measuring equipment

---

## §7.1 Planning of product realization

The organization must plan and develop the processes needed for product realization, consistent
with QMS process requirements (§4.1) and determining, as appropriate: quality objectives and
product requirements, the need for processes/documents/resources specific to the product, required
verification/validation/monitoring/measurement/inspection/test/handling/storage/distribution/
traceability activities and their acceptance criteria, and records needed to provide evidence of
conformity.

For a software-only or software-dominant product built on TrustSC, this planning activity maps
onto a manufacturer choosing which `trustsc-core`/`trustsc-governance` types to populate and how: a
`DeviceContext` (`crates/trustsc-core/src/lib.rs`) fixing the product's identity and `SafetyClass`, a
`ComplianceProgram` fixing which requirements/hazards/verification cases will be tracked, and a
`UiSdkConfig`/`GraphicsProfile` choice fixing whether the product targets Vulkan (Class B) or
Vulkan SC (Class C). `FrameworkBuilder::build()` (`crates/trustsc/src/lib.rs`) refuses to construct a
`Framework` unless these choices are mutually consistent — for example, a Class C `DeviceContext`
paired with a non-Vulkan-SC `UiSdkConfig` fails at construction — which gives the "plan is
consistent with product requirements" half of §7.1 a compile-and-construction-time check for the
UI/device-context slice of planning, though the broader realization plan (verification/validation
strategy, storage, distribution, traceability records) remains the manufacturer's to define.

## §7.2 Customer-related processes

### §7.2.1 Determination of requirements related to product

The organization must determine customer-specified requirements (including delivery/post-delivery
activities), requirements not stated by the customer but necessary for specified/intended use,
applicable regulatory requirements, and any user training needed for the device's intended
performance and safe use.

### §7.2.2 Review of requirements related to product

Before committing to supply a product, the organization must review these requirements, resolve any
differences from what was previously stated, confirm regulatory requirements can be met, and ensure
it has the capability to meet the determined requirements, documenting the review results and any
necessary actions.

### §7.2.3 Communication

The organization must plan and implement effective communication arrangements with customers
regarding product information, inquiries/contracts/orders (including amendments), customer
feedback (including complaints), and advisory notices.

These three sub-clauses describe a manufacturer-facing sales/contract-review/support process
TrustSC plays no role in — it has no customers of its own in the regulatory sense, no
delivery/post-delivery process, and no advisory-notice mechanism. The one loosely-related
observation worth recording: `trustsc_governance::Requirement.source_clause` gives a manufacturer a
place to record *which* customer or regulatory requirement (however determined under §7.2.1) a
given software requirement traces back to, so the review-and-traceability spirit of §7.2.2 has
somewhere to land once requirements reach the software level — but §7.2 itself, its review, and its
communication arrangements are entirely the manufacturer's process to run.

## §7.3 Design and development

This is the sub-clause where ISO 13485 and IEC 62304 (`docs/iec62304/02-development-planning-and-requirements.md`
through `docs/iec62304/04-development-implementation-and-testing.md`) genuinely describe the same
underlying activity from two angles: ISO 13485 §7.3 states the *quality-system* requirements for
design and development control that apply across every product realization domain (mechanical,
electrical, software), while IEC 62304 §5 is the *software-specific* elaboration of those same
control points for a software item. A manufacturer building on TrustSC satisfies both by running
one design and development process, not two parallel ones — see `docs/iec62304/README.md` for the
software-lifecycle framing this corpus otherwise uses, and read this section as the same territory
seen through the QMS lens.

### Design and development planning

The organization must plan and control design and development, determining stages, required
review/verification/validation activities at each stage, responsibilities and authorities,
resources needed, methods to ensure traceability of outputs to inputs, and how outputs will meet
input requirements — updating the plan as design progresses. This is, clause-for-clause, IEC 62304
§5.1's software development planning applied at the whole-product level rather than the software
item alone; see `docs/iec62304/02-development-planning-and-requirements.md#51-software-development-planning`
for the concrete artifacts (ADR trail, CI workflow, `docs/architecture.md`) a manufacturer building
on TrustSC can point to for the software-item slice of this plan.

### Design and development inputs

Inputs relating to product requirements must be determined and records maintained, including
functional/performance/safety/usability requirements as appropriate to the intended use, applicable
regulatory requirements and standards, applicable risk management output, and information from
previous similar designs, with incomplete/ambiguous/conflicting requirements resolved and inputs
reviewed for adequacy and approved.

`trustsc_governance::Requirement { id, title, source_clause, verification_intent }` is the structured
representation of exactly this: `source_clause` ties a requirement back to the standard, hazard, or
customer need that generated it, and `verification_intent` forces a stated verification approach at
the point a requirement is captured, rather than deferred until later. `Hazard.controlled_by`
(non-empty by construction, `Hazard::validate()`) is the mechanism connecting risk management
output — an ISO 14971 hazard, see [`docs/iso14971/`](../iso14971/README.md) — into a design input
requirement, matching this sub-clause's explicit call for risk management output as a design input.

### Design and development outputs

Design and development outputs must be provided in a form enabling verification against inputs and
must be approved before release, and must meet input requirements, provide (or reference) purchasing/
production/service-provision information, contain or reference product acceptance criteria, and
specify characteristics essential for safe and proper use.

For the UI slice of a device, `trustsc-ui-dsl-authoring`'s `.medui` compiler output (a generated
`CompiledScreenPackage`, see `docs/dsl/build-integration.md` and ADR-008/ADR-009) is a design output
in this sense: it is a concrete, versioned artifact derivable from a `.medui` source input, checked
against acceptance criteria at compile time (text-budget containment per ADR-010, safety-critical
requirement binding per ADR-011) before it can be included in a build at all. For the ML inference
slice, a baked `ModelPackage` (`generated/models/<id>/package.json`, ADR-017) plays the same role:
an output that cannot exist without having already passed its own acceptance check (golden
self-test vector reproduction, enforced by `Classifier1D::new()`'s fail-closed startup check).

### Design and development review

At suitable stages, systematic reviews of design and development must be performed per planned
arrangements to evaluate the ability of results to meet requirements and identify problems,
including representatives of functions concerned with the stage being reviewed, with review
results, participants, and product identification recorded.

TrustSC's own analogue, again scoped to *this project's* design and development rather than a
manufacturer's, is the ADR review-and-acceptance process: every design decision that crosses a
trust-zone or compile-time-contract boundary becomes an ADR, and `docs/adr/README.md` requires
`Status: Accepted` before it governs anything — a lightweight but real instance of "review results
recorded" for architectural decisions specifically. A manufacturer's own §7.3 design reviews of a
device built on TrustSC are a separate, broader activity this project does not perform on the
manufacturer's behalf.

### Design and development verification

Verification must confirm design and development outputs meet input requirements, with results and
conclusions (including necessary actions) recorded — and where the intended use requires connection
to or interface with other medical devices, verification must include confirmation that design
outputs meet those interface requirements when connected/interfaced.

Every `Requirement` in a `ComplianceProgram` needs at least one `VerificationCase` referencing it,
or `ComplianceProgram::validate()` fails (`crates/trustsc-governance/src/lib.rs`) — the same rule
IEC 62304 §5.2.4/§5.5-§5.7 states for software verification, applied here as this sub-clause's
requirement that verification results exist and are checkable, not merely asserted. `VerificationCase.method`
is a closed enum (`analysis`, `inspection`, `test`, `demonstration`) matching the standard vocabulary
of verification methods a reviewer would expect a design verification record to use.

### Design and development validation

Validation must be performed per planned arrangements to confirm the resulting product is capable
of meeting requirements for its specified application or intended use, on representative product
under defined operating conditions, prior to release for use (with rationale recorded if that isn't
practicable) — and for devices requiring clinical evaluation or performance evaluation, that
evaluation is part of validation.

TrustSC provides evidence toward *engineering-level* validation of the software components it
supplies, not clinical validation of a finished device (which is out of scope for a library
entirely). `--verify-ui` (ADR-016) — offscreen rendering plus rendered-truth checks against compiled
`GoldenBounds`/`InkContainment` — is the closest thing to validation-under-representative-conditions
this project performs on its own output: it checks that a compiled screen actually renders where
and how it claims to, on the CI-used software rasterizer as well as real hardware, rather than only
checking that the compilation step produced *a* result. A manufacturer's device-level clinical or
performance validation is an entirely separate activity built on top of, not replaced by, this
engineering-level check.

### Design and development transfer

Design and development outputs must be verified as suitable for manufacturing before becoming final
production specifications, and the transfer results recorded.

For a device built on TrustSC, "manufacturing" for the software item is closer to "the build and
release pipeline" than a physical production line: `.github/workflows/ci.yml` running
`cargo build --locked --workspace`, `cargo test --locked --quiet`, and every baker's `verify`
subcommand against committed evidence artifacts is the mechanized check that a design output (a
compiled screen, a baked model, a compiled text package) is reproducible from its reviewed source
before it is treated as ready to ship — the software-transfer analogue of confirming design outputs
are suitable for production.

### Control of design and development changes

Design and development changes must be identified, and their significance to function/performance/
usability/safety/applicable regulatory requirements and to the device's intended use evaluated;
changes must be reviewed/verified/validated as appropriate and approved before implementation, with
results of the change review (including actions taken) recorded.

`trustsc_governance::AuditEvent`'s `Lifecycle` category records every requirement/hazard/verification
addition with a sequence number (`ComplianceProgram::record_event`), and a superseding ADR (rather
than a silent edit to an existing one) is this project's own change-control discipline for
architectural decisions (`docs/iec62304/02-development-planning-and-requirements.md#513-keeping-the-plan-current`
describes the same mechanism from the IEC 62304 angle). Neither performs the significance evaluation
or change approval this sub-clause requires — that judgment belongs to the manufacturer's design
change process — but both give it a recorded trail to operate against.

### Design and development files

For each medical device type or family, a design and development file must be established and
maintained, containing (or referencing) records demonstrating conformity to design and development
requirements and the results of any related changes.

`ComplianceProgram::trace_matrix_export()` and `audit_export()` are exportable slices of exactly
this kind of file for the software item: a requirement-to-verification-to-hazard trace matrix and a
sequenced record of governance-data changes, in a form a manufacturer's broader design and
development file can incorporate by reference rather than transcribe by hand. The file itself —
covering non-software design outputs, physical verification/validation records, and formal change
approvals — remains the manufacturer's to assemble.

## §7.4 Purchasing

### §7.4.1-§7.4.3 Purchasing process, purchasing information, and verification of purchased product

The organization must ensure purchased product conforms to specified purchasing requirements, with
control proportional to the purchased product's effect on subsequent product realization or the
final device, evaluating and selecting suppliers based on their ability to supply product meeting
requirements, monitoring supplier performance, and maintaining supplier evaluation records.
Purchasing information must describe the product to be purchased, including approval requirements
for product/procedures/processes/equipment and personnel qualification, and quality management
system requirements as applicable.

TrustSC's own dependency governance is the closest analogue an incorporating manufacturer will
find, applied to *this project's* third-party dependencies rather than the manufacturer's own
purchased components: `docs/governance/soup-register.toml` records, per dependency, `supplier`,
`repository`, `license`, `usage`, `support_model` ("Community-maintained crate; no medical-device
qualification or vendor support agreement" is the honest, recurring assessment for most entries),
and `risk_controls`. This is a documented supplier/dependency evaluation record in spirit — it
tells a manufacturer's own §7.4 purchasing-control process what TrustSC's transitive dependency
surface looks like and how each entry is risk-controlled — but a manufacturer treating TrustSC
itself as purchased/outsourced software still needs their own §7.4.1 supplier evaluation of
TrustSC as a supplier, which this project cannot perform on the manufacturer's behalf.

## §7.5 Production and service provision

### §7.5.1 Control of production and service provision

Production and service provision must be planned and carried out under controlled conditions:
availability of information specifying product characteristics, work instructions where necessary,
use of suitable equipment, availability/use of monitoring and measuring equipment, implementation of
monitoring/measurement activities, and defined processes for labelling/packaging/release/delivery/
post-delivery activities.

### §7.5.2-§7.5.11 Cleanliness, installation/servicing, sterile-device provisions, process validation, identification, traceability, customer property, preservation

These sub-clauses (cleanliness of product, installation and servicing activities, particular
requirements for sterile devices, validation of production/sterilization processes, identification,
traceability including for implantable devices, customer property, and preservation of product)
describe physical manufacturing and post-manufacture handling activities with no analogue in a
software library. TrustSC is not manufactured, packaged, installed, or serviced in the sense §7.5
describes; a device incorporating TrustSC applies the whole of §7.5 to its own physical/production
reality, entirely outside this project's scope. The one sub-clause worth a narrower note is
traceability (§7.5.9): where a regulation requires unique device identification and traceability
records, a device's software version identity — `DeviceContext.compliance_label()`
(`crates/trustsc-core/src/lib.rs`), formatting `"{product_name} {version} ({safety_class})"` — is a
minimal, generated identity string a manufacturer's broader traceability record could incorporate
for the software component specifically, not a substitute for device-level traceability as a whole.

## §7.6 Control of monitoring and measuring equipment

Equipment needed to provide evidence of product conformity to determined requirements must be
determined, and monitoring and measuring processes/equipment established to ensure they are
consistent with monitoring and measurement requirements; equipment must be calibrated/verified
against traceable standards (or the basis recorded if none exists), adjusted/re-adjusted as
necessary, identified to enable calibration status determination, safeguarded from adjustments that
would invalidate results, and protected from damage/deterioration — with software used in monitoring
and measurement validated before use and revalidated as needed.

This sub-clause's software-validation clause is the most relevant: where a manufacturer uses
software as measuring/monitoring equipment (for example, a tool that measures rendered UI geometry
against a specification), that tool itself needs validation before its output is trusted. TrustSC's
`--verify-ui` (ADR-016) is exactly this kind of measuring tool applied to itself: it validates
rendered pixels against compiled `GoldenBounds`/`ColorHash` expectations, and its own correctness is
checked by CI running it against known-good screens on every push rather than trusting it once and
never re-checking. `tools/trustsc-font-baker`, `trustsc-shader-baker`, and `trustsc-ml-baker`'s `verify`
subcommands are the same pattern for asset-pipeline "measurement": each re-derives a digest from
source and fails if it no longer matches the committed evidence, which is the software-tool
analogue of a calibration check being re-run rather than assumed permanent. One honest gap: CI pins
the Rust toolchain only to `stable` (`dtolnay/rust-toolchain@stable` in `.github/workflows/ci.yml`),
not to an exact compiler version — a manufacturer whose own §7.6-equivalent control requires
traceable, versioned tooling should pin and record the exact toolchain version they build a
regulated release with, rather than relying on TrustSC's own CI configuration for that guarantee.

---

## Related documents

- [Foundations and quality management system](01-foundations-and-qms.md)
- [Management and resources](02-management-and-resources.md)
- [Measurement, analysis and improvement](04-measurement-analysis-improvement.md)
- [IEC 62304 corpus](../iec62304/README.md)
- [AI-Reference index](AI-Reference.md)
- [Citation convention](../governance/citation-convention.md)
