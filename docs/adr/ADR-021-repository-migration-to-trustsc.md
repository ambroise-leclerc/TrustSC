# ADR-021: Repository migration to TrustSC

## Status

Accepted

## Context

The repository was created as **MduX-rust**, a name chosen to distinguish it from the original
C++ `MduX` prototype it started as a sibling of. Two things have made that name obsolete:

- **Scope.** "Medical device UX, in Rust" described the original ambition — a UI library. The
  project is now a medical-device manufacturer framework: trust-zoned workspace (ADR-005),
  byte-verified evidence pipelines (ADR-007), a zero-SOUP ML inference stack (ADR-017), a
  regulatory reference corpus and software-development-file scaffold (ADR-019), and machine-
  checked citation discipline (ADR-020). The UI is one pillar, not the identity.
- **Public provenance hygiene.** GitHub's repository-page contributor sidebar credits
  `Co-authored-by:` trailers found in the head commits of merged pull requests, which GitHub
  retains forever under immutable `refs/pull/N/head` refs — even when every such PR was
  squash-merged and the default branch contains no such trailers (verified: `main` has zero).
  As a result the public repository page lists AI-tool identities from early, pre-convention PR
  branches as "contributors", contradicting this repository's git convention that commits and
  PRs carry no AI attribution. Those refs cannot be deleted by the repository owner, and a
  repository *rename* preserves them; only a fresh repository drops them.

The replacement name, **TrustSC**, was selected after an availability and conflict search
(crates.io, GitHub, npm, domains, USPTO/EUIPO screens): *trust* structurally contains *rust*,
and *SC* reads as both safety-critical and Vulkan SC (ADR-006). Backronym, matching the
architecture pillar-for-pillar: **T**raceable, **R**eproducible, **U**nsafe-free,
**S**afety-classified, **T**oolkit.

## Decision

- **Create a new repository, `ambroise-leclerc/TrustSC`, instead of renaming MduX-rust.** The
  full `main` history is pushed **byte-identical** — same commit objects, same SHAs, same
  author and committer dates. Commit provenance is part of the project's audit record:
  rewriting messages, authorship, or dates (including any backdating to simulate a longer
  history) is rejected outright as incompatible with a project whose value proposition is
  evidence integrity. The development timeline stands as it actually happened.
- **MduX-rust is archived, not deleted**, with a final README pointer to TrustSC. Commit
  messages reference the old repository's PR/issue numbers (`#1`–`#140`); archiving keeps that
  discussion trail resolvable for auditors and future contributors.
- **The `mdux-*` → `trustsc-*` rename of crates, tools, and documentation lands as follow-up
  PRs in the new repository**, each leaving CI green, per the existing stacked-PR convention.
  Until that lands, the `mdux-*` names remain the valid ones; this ADR does not itself rename
  anything.
- **The namespace is reserved at migration time**: the `trustsc` crate name on crates.io and
  the trustsc.dev / trustsc.rs domains, so the public name cannot be squatted between the
  repository becoming visible and the rename PRs landing.

## Consequences

### Positive

- The public contributor list reflects the actual authorship record of `main` — nothing more,
  nothing less — and stays consistent with the no-AI-attribution git convention.
- The project name matches its scope and audience, and the TRUST decode gives auditors and
  newcomers an accurate one-line mental model.
- Clean, collision-free namespace everywhere the project will appear (crates.io, GitHub,
  domains), verified before adoption.

### Negative / limitations

- Stars, watchers, forks, and issue/PR history do not migrate, and — unlike a rename — GitHub
  provides no automatic URL redirects from the old repository. External links to MduX-rust
  keep working only because the old repository stays archived rather than deleted.
- `#NN` references in migrated commit messages autolink to TrustSC's own (initially empty)
  issue numbering; readers must follow them against the archived MduX-rust instead.
- Until the rename PRs land, the repository is named TrustSC while its crates are still named
  `mdux-*`; the mismatch is transitional and tracked by the follow-up work above.

## Related

- [ADR-005](ADR-005-pure-rust-project-boundary-and-dependency-policy.md) — the trust-zone
  architecture the new name refers to
- [ADR-007](ADR-007-compliance-evidence-and-generated-artifact-ownership.md) — the evidence
  discipline that makes provenance non-negotiable
- [ADR-020](ADR-020-ai-interaction-standardization.md) — the agent-interaction and attribution
  conventions the migration restores on the public repository page
- [`AGENTS.md`](../../AGENTS.md) — git conventions (no AI attribution)
