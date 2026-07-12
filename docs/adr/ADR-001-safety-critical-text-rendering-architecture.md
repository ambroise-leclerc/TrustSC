# ADR-001: Safety-critical text rendering architecture

## Status

Accepted

## Context

`TrustSC` needs a text-rendering subsystem that can serve Class B and Class C medical-device use cases without letting Unicode shaping, bidi resolution, font parsing, and atlas generation leak into the safety-critical runtime.

The guiding principle is the same as the reference UI-rendering ADR used for this work: keep high-variability authoring logic outside the safety-critical boundary and keep the runtime boundary narrow, deterministic, and auditable.

## Decision

Split the subsystem into two explicit partitions:

1. **Host-side authoring**
   - font intake and provenance tracking
   - approved string catalog management
   - Unicode normalization, shaping, and bidi resolution
   - glyph rasterization and atlas generation
   - reproducibility verification and evidence generation
2. **Safety-critical runtime**
   - immutable text-package loading
   - fixed-capacity glyph command generation
   - rendering of approved strings and bounded numeric tokens only

The runtime shall not parse fonts, perform shaping, resolve bidi, discover fallback fonts, or allocate in the draw path.

## Consequences

- Runtime behavior stays small enough to validate and reason about for Class C use.
- Complex Unicode and font-processing dependencies remain in host tooling where they can be version-pinned and re-run reproducibly.
- The system only supports arbitrary Unicode through the offline approval pipeline; it does not support arbitrary runtime-provided strings in the safety-critical boundary.
