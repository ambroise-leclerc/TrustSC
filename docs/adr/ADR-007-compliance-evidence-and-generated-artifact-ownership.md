# ADR-007: Compliance evidence and generated artifact ownership

## Status

Accepted

## Context

The workspace already produces multiple evidence-like outputs: governance exports, audit logs, release summaries, compiled text packages, and text reproducibility digests. More generated outputs will follow as Vulkan and Vulkan SC pipeline assets mature.

Without a clear ownership and lifecycle model, generated files can drift into acting like hand-maintained source documents, which weakens traceability and makes release evidence harder to reproduce.

## Decision

- Treat authored product and compliance inputs as the source of truth: device metadata, requirements, hazards, verification cases, approved text catalogs, manifests, profile settings, and generation configuration.
- Treat vendored approved assets such as `assets/fonts/roboto/` as reviewed source inputs. Their provenance records, license notices, and Yocto handoff metadata stay with the source asset and are maintained like any other controlled input.
- Treat trace matrices, audit exports, release summaries, compiled text packages, determinism digests, and pipeline outputs as **generated evidence artifacts** that are regenerated from pinned inputs and are never edited by hand.
- Keep generated Roboto bake outputs such as `generated/fonts/roboto-regular-16px/package.json` and `report.json` in the generated-artifact class: they may be checked in for reviewable evidence, but changes come from rerunning the host baker, not manual patching.
- Ownership stays with the subsystem that defines the contract:
  1. `mdux-governance` owns compliance exports such as trace matrices, audit logs, and release evidence summaries.
  2. `mdux-text-schema` owns the schema for compiled text evidence, and `mdux-text-authoring` owns generation of compiled text packages and reproducibility data.
  3. `mdux-ui` owns profile constraints, resource-budget expectations, and the contract for Vulkan or Vulkan SC rendering artifacts.
  4. `mdux` may aggregate subsystem outputs into framework-level release bundles, but it does not replace subsystem ownership of the underlying evidence.
- Yocto packaging metadata embedded in source manifests remains input metadata for future packaging automation; it does not reclassify host-only tools or generated reports as target-runtime deliverables.
- Transient build outputs stay disposable. Approved release evidence is archived as immutable output attached to the corresponding build, release, or regulated record, and changes to inputs require regeneration rather than manual patching.

## Consequences

- Auditors and maintainers can distinguish reviewed inputs from reproducible generated outputs.
- Each crate family has explicit accountability for the evidence it produces and validates.
- Release packages can be rebuilt from the repository state and pinned toolchain instead of relying on stale edited artifacts.
