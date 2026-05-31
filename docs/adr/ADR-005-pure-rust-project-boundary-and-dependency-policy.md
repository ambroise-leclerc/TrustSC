# ADR-005: Pure-Rust project boundary and dependency policy

## Status

Accepted

## Context

`MduX-rust` replaces the original C++ framework with a Rust workspace intended to keep the regulated runtime, governance model, and asset contracts auditable. The core crates already expose safe Rust APIs and forbid `unsafe`, while outer examples may still need platform bindings such as Vulkan window integration.

Without an explicit boundary, native SDK wrappers, generated bindings, or foreign handles could leak into the compliance-critical crates and make the framework contract harder to review and validate.

## Decision

- The governed workspace crates (`mdux-core`, `mdux-governance`, `mdux-ui`, `mdux`, `mdux-text-schema`, `mdux-text-authoring`, and `mdux-text-runtime`) are pure Rust implementation boundaries with safe Rust public APIs and no C/C++ source or bindgen-generated interfaces.
- These crates may depend only on workspace crates or version-pinned Rust dependencies whose determinism, maintenance, and licensing can be reviewed through the normal workspace process.
- `unsafe` code, native SDK wrappers, and FFI-facing types are allowed only in edge adapters such as platform examples, harnesses, or future integration crates that translate into owned Rust data before crossing into the governed API boundary.
- Foreign pointers, handles, and ABI-specific structs shall not appear in the public interfaces of the governed crates. If a new capability cannot satisfy that rule, it requires a new ADR or a separate adapter boundary.
- Host-side helper binaries under `tools/` (including `tools/mdux-font-baker`) are outside the regulated runtime boundary. They may use additional reviewed third-party crates only when those dependencies are pinned, recorded in the SOUP register, and kept behind a file-based handoff into governed manifests or generated evidence.
- Host-only tooling and its dependencies shall not be linked into device/runtime crates, installed into future Yocto target images, or treated as part of the validated runtime software item.

## Consequences

- The replacement framework keeps a narrow, auditable Rust-only core for Class B and Class C reviews.
- Platform experimentation such as host Vulkan bring-up can continue without redefining the regulated framework contract.
- Dependency additions to governed crates now require explicit scrutiny for reproducibility and compliance impact, not just functional convenience.
