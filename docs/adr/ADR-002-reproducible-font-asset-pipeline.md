# ADR-002: Reproducible font asset pipeline

## Status

Accepted

## Context

Font files, approved strings, Unicode data versions, and atlas-generation rules all influence rendered output. For a safety-oriented SDK, the same approved inputs must lead to the same atlas textures and package bytes.

## Decision

Adopt an explicit host-side asset pipeline with these stages:

1. **Font intake**
   - register source font files
   - record SHA-256 digests, provenance, and approval metadata
2. **Catalog compilation**
   - validate approved/localized strings
   - normalize text and resolve bidi/shaping offline
3. **Atlas generation**
   - rasterize glyphs with a version-pinned toolchain
   - pack glyphs with a deterministic placement algorithm
4. **Package verification**
   - rebuild packages from the same manifest
   - compare atlas hashes and final package hashes byte-for-byte

Tooling inputs must be fully pinned: Rust toolchain, `Cargo.lock`, Unicode data version, source font digests, and package schema version.
- The default bootstrap asset is the vendored Roboto Regular face under `assets/fonts/roboto/`; its `font-manifest.toml`, `provenance.toml`, `LICENSE`, and `NOTICE` remain adjacent to the TTF so provenance, ownership, and licensing travel with the approved source asset.
- Source inputs live under `assets/`, while baked packages, reports, and other deterministic outputs live under `generated/`. Generated outputs are rebuilt from pinned inputs and are not edited as source documents.
- Font manifests may carry Yocto-facing handoff metadata such as package name, install subdirectory, license expression, and license file list so future recipes can install approved font assets and notices without promoting the host baker into the target image.
- Host-only bake helpers such as `tools/mdux-font-baker` may use separately reviewed SOUP crates, but those dependencies remain outside the runtime boundary; only the reviewed asset manifests and generated package bytes cross into release evidence.

## Consequences

- Rebuilds can produce evidence that text assets are reproducible.
- Unapproved font changes are caught as manifest or hash mismatches.
- Atlas layout must avoid nondeterministic heuristics such as hash-order-driven packing.
