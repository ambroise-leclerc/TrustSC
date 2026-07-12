# IEC 81001-5-1: Secure design and secure implementation

## Module overview

This module covers two adjacent activity groups: secure design (architectural and detailed design
decisions made to reduce and contain the consequences of a security compromise) and secure
implementation (coding practices that avoid introducing exploitable defects). Both activity groups
are, in IEC 81001-5-1's integration model described in module 01, additional constraints layered onto
the same design and implementation activities IEC 62304 §5.3-§5.5 already require — not a separate
design or coding pass. MduX-rust's trust-zone architecture and its `#![forbid(unsafe_code)]` policy
are the two mechanisms this module leans on most heavily, because they are genuinely, verifiably true
of the repository today rather than aspirational.

**Key areas covered:**
- Secure design principles: attack surface minimization, defense in depth, least privilege
- The governed/adapter/tools trust-zone boundary as a secure design control (ADR-005)
- `#![forbid(unsafe_code)]` and memory safety as secure implementation controls
- Secure implementation practices beyond memory safety: input validation, deterministic build evidence
- Honest limits: what this architecture does not, by itself, secure

---

## §6 (approx.) Secure design principles

Secure design is usually organized around a small set of recurring principles: minimize attack
surface (expose as little as possible to untrusted input), defense in depth (no single control is the
only thing standing between an attacker and harm), least privilege (a component gets only the access
it needs to do its job), and secure defaults (a misconfiguration should fail toward the safer state,
not the more permissive one). These are general principles, not clause text specific to IEC 81001-5-1
or IEC 62443-4-1, and this module applies them to MduX-rust's actual architecture rather than
restating them abstractly.

### Attack surface minimization via the trust-zone boundary

ADR-005's governed/adapter/tools split (see
[`../../docs/architecture.md`](../architecture.md) and
[`ADR-005`](../adr/ADR-005-pure-rust-project-boundary-and-dependency-policy.md)) is, read as a
security control, an attack-surface-minimization design: the governed crates (`mdux-core`,
`mdux-governance`, `mdux-ui`, `mdux`, `mdux-text-schema`, `mdux-text-authoring`, `mdux-text-runtime`,
`mdux-ml-schema`, `mdux-ml-authoring`, `mdux-ml-runtime`, `mdux-ui-dsl-authoring`) may depend only on
each other or on version-pinned, reviewable Rust crates, and their public APIs may never expose an FFI
type, a native SDK handle, or bindgen output. Concretely, this means the code that is reachable from
any untrusted or semi-trusted input the SDK processes — a `.medui` screen file, a compiled text
package, an ML model package — is confined to a deliberately narrow, `unsafe`-free surface that a
reviewer (human or automated) can examine in full, rather than an entire transitive dependency graph
including native windowing and graphics bindings.

`adapters/mdux-vulkan-winit`, by contrast, is where `unsafe`, `ash`/`ash-window`/`raw-window-handle`/
`winit`, and all native Vulkan/OS interaction is permitted — but ADR-012's boundary rule constrains
even this: every public function on that crate's `App` type must take or return owned Rust data
already defined by a governed crate, so a foreign handle or FFI type can never leak back across the
boundary into governed code. This is a least-privilege argument applied to trust rather than to OS
permissions: the governed core is never given the ability to reach into the native layer beyond what
the adapter chooses to expose as plain data.

### Defense in depth: multiple independent controls, not one

No single mechanism in MduX-rust is asked to carry the whole secure-design burden by itself.
`#![forbid(unsafe_code)]` (below) rules out a whole defect class in the governed crates; the trust-
zone boundary confines the remaining `unsafe` surface to a small, adapter-only area; the SOUP register
(module 02) tracks what third-party code exists in each zone; and byte-verified evidence generation
(ADR-007) independently checks that what was built matches what a reviewed input and recipe should
produce. Each of these would catch a different class of problem; none of them substitutes for the
others.

## §6 (approx.) `#![forbid(unsafe_code)]` as a secure implementation control

Every governed crate in the workspace carries `#![forbid(unsafe_code)]` as its first line
(`crates/mdux-core/src/lib.rs`, `crates/mdux-governance/src/lib.rs`, `crates/mdux-ui/src/lib.rs`,
`crates/mdux/src/lib.rs`, `crates/mdux-text-schema/src/lib.rs`, `crates/mdux-text-authoring/src/lib.rs`,
`crates/mdux-text-runtime/src/lib.rs`, `crates/mdux-ml-schema/src/lib.rs`,
`crates/mdux-ml-authoring/src/lib.rs`, `crates/mdux-ml-runtime/src/lib.rs`,
`crates/mdux-ui-dsl-authoring/src/lib.rs`, `crates/mdux-ui-verify/src/lib.rs`). This is a compiler-
enforced, not merely a documented, guarantee: Rust's `unsafe` keyword is the mechanism through which a
whole class of memory-safety defects (buffer overruns, use-after-free, data races on shared mutable
state) become possible, and forbidding it means the compiler itself rejects a change that would
introduce one, rather than relying on code review to catch it. For a secure implementation practice,
this is about as strong a static guarantee as a language can offer without formal verification, and it
applies to every line of governed code that a `.medui` file, a compiled text/ML package, or any other
externally-influenced input passes through.

This guarantee is real but scoped: it says nothing about logic errors, panics on malformed input, or
`unsafe` code in the adapter/tools zones (which is exactly why those zones exist as a separate,
smaller review surface rather than being treated as equally safe). A manufacturer's secure
implementation review of code built on MduX-rust should treat the `#![forbid(unsafe_code)]` boundary
as a strong floor for the governed crates, not as a claim that covers `adapters/mdux-vulkan-winit` or
`tools/`.

## §6 (approx.) Secure implementation practices beyond memory safety

### Input validation at package/model boundaries

`mdux-text-runtime::TextRuntime::new()` and `mdux-ml-runtime::Classifier1D::new()` both validate the
compiled package they are given once at construction — a text package's structural invariants, and an
ML package's golden self-test vectors respectively — and refuse to proceed on failure (see
`../architecture.md` and the ADR-017 summary in `../adr/README.md`). Read as a secure implementation
practice rather than purely a correctness one, this is input validation at a trust boundary: a
compiled package is untrusted data as far as the runtime is concerned, even though it was produced by
the project's own host tooling, and the runtime does not assume it is well-formed just because it
came from a `bake` step.

### Deterministic, strictly-ordered arithmetic

`mdux-ml-runtime`'s kernels are deliberately plain, strictly-ordered scalar loops — no SIMD, no
`f32::mul_add`/FMA — specifically so host-computed golden vectors reproduce bit-for-bit on-device
(ADR-017). This is primarily a safety/correctness property, but it has a secure-implementation
dimension too: it removes a class of platform- or compiler-version-dependent floating-point behavior
that could otherwise make the same model package behave subtly differently on different build
targets, which would complicate reasoning about whether an observed behavior difference is a genuine
defect, a miscompilation, or a tampering signal.

### Build-pipeline integrity via byte-verified evidence

ADR-007's `bake`/`verify` pattern — a host-only `tools/*-baker` binary produces a `package.json` +
`report.json` pair from a reviewed source input and recipe, and CI's `verify` step re-derives the
digest and fails on mismatch — is a supply-chain integrity control on the build pipeline itself: it
does not merely document what was built, it makes an unreviewed or accidental change to a generated
evidence artifact (a font atlas, a shader, an ML model package) fail CI rather than silently ship. For
secure implementation purposes this is the closest existing MduX-rust mechanism to a build-
reproducibility / tamper-evidence control, applied uniformly across every asset pipeline in the repo
(`tools/mdux-font-baker`, `tools/mdux-image-baker`, `tools/mdux-shader-baker`, `tools/mdux-ml-baker`).

## §6 (approx.) Honest limits

None of the above should be read as "MduX-rust is secure by construction." `#![forbid(unsafe_code)]`
does not prevent logic errors, denial-of-service via malformed input causing a panic, or resource
exhaustion; the trust-zone boundary narrows the *review surface*, it does not eliminate the need to
review `adapters/mdux-vulkan-winit` and `tools/` — it only makes clear which zone a given piece of code
lives in and what obligations attach to that zone; and byte-verified evidence detects that a build
*matches a committed artifact*, it cannot by itself detect that the committed artifact was correct in
the first place (a compromised source asset baked correctly still produces a byte-identical, compromised
result). A manufacturer's secure design review should treat these mechanisms as real, load-bearing
controls, not as a substitute for their own threat modeling, code review, and penetration testing of
the device built on top of MduX-rust.

---

## Related documents

- [Security risk management](02-security-risk-management.md)
- [Security verification and update management](04-security-verification-and-update-management.md)
- [Architecture (trust-zone boundary in full)](../architecture.md)
- [ADR-005](../adr/ADR-005-pure-rust-project-boundary-and-dependency-policy.md)
- [ADR-007](../adr/ADR-007-compliance-evidence-and-generated-artifact-ownership.md)
- [ADR-012](../adr/ADR-012-presentation-adapter-crates-and-shader-artifacts.md)
- [AI-Reference index](AI-Reference.md)
- [Citation convention](../governance/citation-convention.md)
