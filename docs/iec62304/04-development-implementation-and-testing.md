# IEC 62304: Implementation, integration, system testing, and release

## Module overview

The remaining four sub-clauses of §5: writing and unit-verifying software units (§5.5), integrating
them and testing the integration (§5.6), testing the whole system against its requirements (§5.7),
and releasing it (§5.8). This module also covers where TrustSC's evidence-generation pattern
(ADR-007) sits relative to each sub-clause, since it is the project's primary answer to "how is this
verified."

**Key areas covered:**
- Unit implementation and unit verification
- Software integration and integration testing
- Software system testing against requirements
- Release criteria and release documentation

---

## §5.5 Software unit implementation and verification

### §5.5.1 Implement each software unit

Implementation follows the coding standards fixed at the architecture level (module 03) —
`#![forbid(unsafe_code)]` in every governed crate — and the governed/adapter/host-only boundary rule
that determines which zone new code belongs in before it's written (see `docs/architecture.md`'s
"Three trust zones" section). CI does not currently run `clippy` or another lint gate — see
`.github/workflows/ci.yml` for exactly what it does run.

### §5.5.2 Establish unit verification process

Unit-level correctness is verified by `cargo test`, scoped per crate. `trustsc-ml-runtime`'s
`Classifier1D::new()` additionally performs a unit-verification step *at runtime*, not only at
build time: it re-runs every golden self-test vector baked by `trustsc-ml-authoring` and fails closed
on any bit-mismatch, catching miscompilation or target floating-point drift that a build-time test
suite alone cannot (ADR-017).

### §5.5.3 Unit verification

Beyond `cargo test`, generated-evidence crates verify their own committed artifacts by
byte-comparison: `tools/trustsc-font-baker`, `tools/trustsc-shader-baker`, and `tools/trustsc-ml-baker` each
expose a `verify` subcommand that re-derives a `report.json` from its source input and fails if the
digest doesn't match what's committed (ADR-007). This is unit verification of the *baking* process,
distinct from testing the runtime crates that consume the baked output.

## §5.6 Software integration and integration testing

### §5.6.1 Integrate software units

`FrameworkBuilder` (`crates/trustsc/src/lib.rs`) is the composition root where `DeviceContext` +
`ComplianceProgram` + `UiSdkConfig` + `UiComponent`s are wired together — the point at which
previously independently-developed governed crates become one `Framework`.

### §5.6.2 Verify software integration

`FrameworkBuilder`'s cross-validation (rejecting a Class C device that isn't configured for the
Vulkan SC profile, rejecting a UI component referencing a nonexistent requirement) is integration
verification enforced at construction time rather than left to a separate manual test phase. The
example applications (`hello_world`, `class_b_device`, `class_c_monitor`, `class_c_vulkansc_device`)
each exercise a full integration path end-to-end, including `--headless-smoke` runs in CI.

## §5.7 Software system testing

### §5.7.1 Establish tests for software requirements

`--verify-ui` (ADR-016) renders a screen offscreen and checks rendered ink against compiled bounds
(`GoldenBounds`/`InkContainment`) — a system-level test that a requirement's UI manifestation
actually renders where and how the compiled screen package says it should, on the CI-used lavapipe
software rasterizer as well as real hardware.

### §5.7.2 Use of software problem resolution process

Failures found during system testing are logged as `trustsc_governance::ProblemReport`s and flow
through module 08 (Problem resolution process) rather than being fixed silently outside that record.

## §5.8 Software release

### §5.8.1 Ensure verification/anomaly resolution is complete

`ComplianceProgram::validate()` is the machine-checkable release gate for the governance data: every
requirement has at least one verification case, every verification case references a real
requirement (no orphans), and a Class C device has at least one recorded hazard. A device whose
`ComplianceProgram` fails `validate()` is not release-ready by this project's model.

### §5.8.2 Document known residual anomalies

`ProblemReport.closed: bool` distinguishes open from resolved problem reports; an unresolved
`ProblemReport` at release time is the residual-anomaly record §5.8.2 asks for.

### §5.8.3 Document released versions

`DeviceContext.compliance_label()` (`crates/trustsc-core/src/lib.rs`) formats
`"{product_name} {version} ({safety_class})"` — the minimal identity string a release record needs,
generated from the same typed data used throughout the compliance program rather than hand-typed
separately.

### §5.8.4 Ensure activities and documentation are complete before release

`ComplianceProgram::release_evidence_summary()` produces a single-line summary
(`device=... class=... requirements=... hazards=... verifications=... problems=... audit_events=...`)
intended as a release-readiness snapshot a human or CI gate can check at a glance.

---

## Related documents

- [Development design](03-development-design.md)
- [Maintenance process](05-maintenance-process.md)
- [Risk management process](06-risk-management-process.md)
- [Problem resolution process](08-problem-resolution-process.md)
- [AI-Reference index](AI-Reference.md)
- [Citation convention](../governance/citation-convention.md)
