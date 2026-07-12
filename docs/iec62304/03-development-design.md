# IEC 62304: Software architectural and detailed design

## Module overview

§5.3 (architectural design) and §5.4 (detailed design) turn approved requirements into a structured
software design before implementation starts. Architectural design fixes the segregation and
interface boundaries a device relies on for safety; detailed design refines each unit to a level
implementable and independently verifiable.

**Key areas covered:**
- Transforming requirements into a software architecture
- Segregation of software items (and how that affects each item's risk-relevant classification)
- SOUP identification at the architecture level
- Detailed design of software units

---

## §5.3 Software architectural design

### §5.3.1 Transform requirements into an architecture

The architecture must identify the software items composing the system and their interfaces —
including with SOUP. TrustSC's own architecture (`docs/architecture.md`) is exactly this: a
three-zone split (`crates/` governed, `adapters/` edge, `tools/` host-only, ADR-005) with each
zone's cross-boundary interface stated as a rule ("no FFI types, native SDK handles, or bindgen
output may appear in a governed crate's public API"; "every public adapter function must take or
return owned data already defined by a governed crate").

### §5.3.2 Develop an architecture for the interfaces of software items

Each governed-crate boundary in TrustSC's crate map (see `docs/architecture.md`'s "Crate map"
section) is an
interface in this sense: `trustsc-core` → `trustsc-governance` → `trustsc-ui`/`trustsc-text-*`/`trustsc-ml-*` → the
`trustsc` facade → `adapters/trustsc-vulkan-winit`. ADR-012 formalizes the adapter-side half of this
(presentation adapter crates and their shader-artifact interface).

### §5.3.3 Identify segregation necessary for risk control

Where segregating software items reduces risk, the architecture should state the segregation and
how it's verified. TrustSC's trust-zone split *is* a segregation used for risk control: `unsafe`
code and native SDK bindings are confined to `adapters/`, so the governed crates that implement UI
policy, text layout, and ML inference can be reviewed and reasoned about as safe Rust in isolation.
`#![forbid(unsafe_code)]` on every governed crate makes this segregation compiler-enforced rather
than convention-based — see `docs/iec62304/06-risk-management-process.md` for how this connects to
hazard analysis.

### §5.3.4 Identify SOUP items

Every third-party dependency used inside a governed or adapter crate, or by host-only tooling that
produces a committed artifact, is recorded in `docs/governance/soup-register.toml` with its
supplier, license, integration path, and pinning mechanism. See `docs/iec62304/08-problem-resolution-process.md`
for the SOUP-anomaly-tracking half of this (AMD1:2015's main addition to §5.3.4/§7).

### §5.3.5 Verify the architectural design

Architectural review happens through this project's ADR process — each ADR states not just a
decision but its consequences and the alternatives considered, giving a reviewer a record to verify
against, and `docs/adr/README.md` requires every ADR to be `Status: Accepted` before it governs
anything.

## §5.4 Software detailed design

### §5.4.1 Refine the software architecture into a detailed design

Each governed crate's internal module structure is the detailed-design layer beneath the
architectural interfaces above — e.g. `trustsc-ml-runtime`'s `Classifier1D<'a, MAX_UNITS, MAX_OUT>`
with its strictly-ordered scalar Dense/Conv1D/pooling/activation kernels (ADR-017), or
`trustsc-text-runtime`'s no-allocation `TextRuntime`/`GlyphDrawCommand` consumer of a pre-compiled
`TextPackage` (ADR-001/ADR-003).

### §5.4.2 Develop a detailed design for interfaces

Public function signatures on governed types are the interface-level detailed design — e.g.
`FrameworkBuilder::with_screen(&'static CompiledScreenPackage)` (`crates/trustsc/src/lib.rs`), which
cross-validates a Class C device against the Vulkan SC profile requirement and rejects a UI
component referencing a requirement that doesn't exist in the compliance program.

### §5.4.3 Verify the detailed design

Unit tests exercising each governed crate's public API stand in for a dedicated detailed-design
review record; `cargo test` run per `.github/workflows/ci.yml` is the mechanized form of this
verification, executed on every push rather than only at a milestone review.

---

## Related documents

- [Development planning and requirements](02-development-planning-and-requirements.md)
- [Implementation and testing](04-development-implementation-and-testing.md)
- [Risk management process](06-risk-management-process.md)
- [Configuration management process](07-configuration-management-process.md)
- [AI-Reference index](AI-Reference.md)
- [Citation convention](../governance/citation-convention.md)
