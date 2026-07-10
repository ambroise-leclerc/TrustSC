# ADR-017: Zero-SOUP machine-learning inference pipeline

## Status

Accepted

## Context

Manufacturers building on MduX-rust want to embed learned models — starting with signal-classification
use cases such as classifying an EEG spectral stream into depth-of-anesthesia states, the domain of the
NeuroSense 500 example (`examples/class_c_monitor`) — without pulling ONNX Runtime, PyTorch, or any
other large, unaudited native inference stack into a Class B/C device. At the same time, teams need a
low-cost way to prototype with openly available pretrained weights (e.g. downloaded from Hugging Face)
before an organization has its own qualified training data, and then to move to production **without
rewriting or re-validating the inference engine** — only the weights change.

This is the same shape of problem ADR-001 already solved for text: keep high-variability,
allocation-heavy authoring logic (Unicode shaping, font rasterization) out of the safety-critical
runtime, and let the runtime consume only an immutable, pre-validated artifact. ADR-005 and ADR-012
establish the governed/adapter/tools trust-zone split and the "host-only tool bakes committed,
byte-verified evidence" pattern (`tools/mdux-font-baker` → `generated/fonts/...`). This ADR applies both
to machine-learning inference.

## Decision

1. **Split the ML subsystem into three governed crates, mirroring the text pipeline (ADR-001):**
   - `mdux-ml-schema` — the shared contract: `ModelPackage`, layer/tensor types, golden self-test
     vectors, and a `DeterminismEvidence` fingerprint. No I/O, no parsing logic. Depends only on
     `mdux-core`.
   - `mdux-ml-authoring` — host-side: imports weights (safetensors, hand-parsed — see point 6),
     validates the declared architecture, deterministically compiles a `ModelPackage`, and generates
     golden input/output vectors by running the package through the same kernels the runtime uses.
   - `mdux-ml-runtime` — device-side: an immutable-package inference engine (`Classifier1D`) doing only
     Dense, Conv1D, pooling, and elementwise-activation arithmetic in safe, `#![forbid(unsafe_code)]`
     Rust. No file I/O, no dynamic graph construction, no heap allocation in `predict`.
2. **Weights are data, not code.** A `ModelPackage` is authored once, offline, by
   `tools/mdux-ml-baker` (mirroring `tools/mdux-font-baker`) and committed as evidence under
   `generated/models/<id>/{package.json,report.json}` — e.g. `generated/models/eeg-demo/` for the
   NeuroSense 500 demonstrator — byte-verified by CI exactly like font and shader evidence. The
   device-side application embeds the package via generated Rust source
   (`mdux_build::ModelPackage` + `mdux::include_model!()`), the same JSON-to-Rust codegen path already
   used for text and image packages. **Swapping a manufacturer's own clinically-qualified weights for
   the Hugging Face demonstrator weights means re-running the baker against a different recipe and
   regenerating `package.json` — zero change to `mdux-ml-runtime` or to application source.** This is
   the two-phase workflow: Phase 1 (demonstrator) imports open weights; Phase 2 (production) imports the
   manufacturer's own trained, clinically-validated, or synthetically-generated weights through the
   identical pipeline.
3. **Determinism is enforced by strictly-ordered scalar arithmetic, not merely by pinning inputs.**
   Every kernel in `mdux-ml-runtime` accumulates in a single, fixed, documented order (e.g. Conv1D sums
   over input channel then kernel tap; Dense sums over input feature) using plain `f32` multiply-then-add
   — never `f32::mul_add`/FMA and never SIMD intrinsics, both of which can produce different rounding
   than the equivalent scalar sequence. This is what lets `mdux-ml-authoring` compute "golden" reference
   outputs on the host and have `mdux-ml-runtime` reproduce them bit-for-bit on the device.
4. **Golden vectors are baked into the package and self-tested at runtime construction.**
   `ModelPackage.golden_vectors` holds host-computed input→output pairs. `Classifier1D::new()` re-runs
   every golden vector and fails closed (returns an error, refuses to construct) if any output diverges
   from the recorded expectation. This is a genuine Class C safety control: it detects toolchain
   miscompilation, target floating-point drift, or a corrupted/mismatched package before the device ever
   classifies a real signal — the ML analogue of `TextRuntime::new()` validating a text package once at
   startup (ADR-003).
5. **v1 scope is `f32` tensors and a small kernel set: `Dense`, `Conv1D`, `MaxPool1D`/`AvgPool1D`,
   `Flatten`, and the `Relu`/`Sigmoid`/`Softmax` activations** — sufficient for lightweight MLP and 1D-CNN
   signal classifiers. Quantized (`int8`) inference is out of scope for v1 and is deferred to a future
   ADR; `mdux-ml-schema` rejects any package declaring a dtype other than `F32`.
6. **Weight import avoids adding runtime-adjacent SOUP.** The safetensors format is a `u64` header
   length, a JSON header describing each tensor's name/dtype/byte-range, and raw tensor bytes.
   `mdux-ml-authoring` hand-parses this directly using the already-registered `serde_json`, rather than
   pulling in a dedicated `safetensors` crate. `import` is a host-only, offline authoring command — it
   never runs in CI and never touches a device/runtime crate, so a malformed or hostile input file can
   never affect a build.

## Consequences

- No ONNX Runtime, PyTorch, `tract`, or other native/foreign inference stack is ever linked into a
  device crate; the entire inference engine is auditable, from-scratch Rust under this project's design
  control, consistent with the "Zero SOUP at runtime" goal.
- A demonstrator built against Hugging Face weights and a production build against clinically-qualified
  weights are byte-identical in every crate except the committed `generated/models/<id>/package.json` —
  this is the central selling point for lowering MedTech prototyping cost without reopening software
  validation at industrialization time.
- The golden-vector self-test adds a small, bounded amount of startup work (not per-frame) in exchange
  for a concrete, auditable defense against silent numerical drift.
- `ModelPackage` schema changes are controlled the same way `TextPackage` changes are (ADR-003): they
  directly affect the validated runtime contract and require re-baking and re-verifying every committed
  package.
- Model complexity is deliberately bounded by v1's kernel set; teams needing larger architectures
  (recurrent layers, attention, int8 quantization) require a follow-up ADR rather than an unreviewed
  runtime extension.
