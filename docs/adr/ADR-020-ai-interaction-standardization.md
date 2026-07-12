# ADR-020: AI interaction standardization — AGENTS.md, skills, MCP policy, and citation linting

## Status

Accepted

## Context

ADR-019 built the regulatory reference corpus to be "LLM- and human-navigable" and explicitly
called out the C++ prototype's discovery gap: its regulatory docs existed but no agent
instruction file pointed at them. This repository half-closed that gap by referencing the corpus
from `CLAUDE.md` — but `CLAUDE.md` is gitignored (a personal, single-tool, single-machine file),
so the project itself still shipped **no** committed agent instructions at all. Three further
gaps followed from that:

- No standardized instruction file readable by the broader agent tooling ecosystem (the
  `AGENTS.md` convention is read by Codex, Cursor, Gemini CLI, Zed, Claude Code, and others).
- No packaged, task-scoped expertise: the repo's specialized protocols (citation convention,
  bake/verify evidence discipline, MedUI DSL rules, SDF document patterns) lived only in long
  reference docs, with nothing selecting the right protocol at the right moment.
- The citation contract agents are asked to honor — exact citation keys, schema-conformant
  `Justification` blocks, real `evidence_refs` paths — was documented in
  `docs/governance/citation-convention.md` but machine-checked nowhere, so drift (a typo'd
  clause, a dangling evidence path, a duplicate `JUS-` id) would only be caught by a human
  reader.
- No recorded evaluation of MCP (Model Context Protocol) servers, leaving "should we configure
  MCP?" to be re-asked ad hoc.

## Decision

- **`AGENTS.md` (repo root, committed) is the canonical, tool-neutral agent instruction file.**
  It carries the trust-zone table, the command set (mirroring `.github/workflows/ci.yml`), the
  regulatory-corpus usage protocol (navigation, citation keys, `Justification` objects, the
  no-reproduction rule), coding/artifact rules, and git conventions. It is the map, not the
  territory: it links into `docs/` rather than duplicating it. `CLAUDE.md` remains gitignored
  and becomes a thin personal overlay that imports `AGENTS.md`; nothing project-shared may live
  only in a gitignored file.
- **Task-scoped expertise ships as Agent Skills under `.claude/skills/`** (committed), in the
  open `SKILL.md` format (YAML `name`/`description` frontmatter + markdown body): four skills —
  `regulatory-citations`, `evidence-pipeline`, `medui-authoring`, `sdf-documents`. Each is a
  short operational recipe pointing into the authoritative docs (progressive disclosure), so the
  underlying references stay single-sourced. The format is markdown-first: tools without skill
  support (and humans) read them as ordinary documentation.
- **`tools/trustsc-docs-lint` machine-checks the citation contract in CI.** A host-only tool (ADR-005
  tools zone, no new dependencies — `serde_json` only, following `trustsc-ml-baker`'s hand-parsing
  precedent) that scans `docs/**/*.md` and `software_development_file/**/*.md` and fails the
  build on: a citation-key-shaped string whose standard id + edition year is not one of the five
  pinned identifiers; a cited clause number absent from that standard's `AI-Reference.md` index
  (with prefix/range awareness); a `Justification` block that fails the structural rules of
  `justification.schema.json`; a duplicate `justification_id`; or an `evidence_refs` path that
  does not exist in the repository. CI runs `cargo run --locked -q -p trustsc-docs-lint -- check`
  alongside the existing evidence `verify` steps.
- **No MCP servers are adopted, and no `.mcp.json` is committed.** Evaluation of the candidate
  categories:
  - *GitHub MCP server* — redundant: the `gh` CLI already covers PR/issue/API workflows used
    here, with finer-grained, auditable invocations.
  - *Filesystem/git servers* — redundant with the file and shell tools built into every agent
    harness this repo targets.
  - *Dependency-documentation servers (docs.rs / crates.io / Context7-style)* — marginal and
    mildly counterproductive: ADR-005's dependency policy makes new third-party crates rare,
    deliberate, human-reviewed SOUP-register events; a live crate-docs feed optimizes for the
    casual dependency addition this project is designed to resist.
  - *A custom TrustSC governance server* (querying the trace matrix, SOUP register, audit trail) —
    the only genuinely interesting candidate, but premature: `trustsc-governance` still exports
    pipe-delimited text, and ADR-019 already defers the `serde` JSON export it would build on.
    Re-evaluate once that export exists.

## Consequences

### Positive

- Any agent (or new contributor) cloning the repo gets the architecture rules, command set, and
  regulatory protocol from a committed, tool-neutral file — ADR-019's discovery gap is fully
  closed, for every tool rather than one.
- The citation convention is now enforced, not just documented: a broken clause reference or
  dangling evidence path fails CI the same way a drifted font atlas does, extending the ADR-007
  "committed evidence is machine-verified" principle to the regulatory prose itself.
- Skills give agents the right protocol at the right moment without inflating every session's
  base context, and stay useful as plain markdown outside skill-aware tools.
- The MCP question has a recorded answer with re-evaluation criteria, instead of being re-argued
  per contributor.

### Negative / limitations

- `trustsc-docs-lint` validates structure, not truth: it cannot check that a clause's short title
  matches the real standard, that a rationale is sound, or that prose stays clear of
  paraphrasing normative text — those remain human-review duties (the linter's clause-number
  check is only as good as the `AI-Reference.md` indexes it trusts).
- Two instruction surfaces (`AGENTS.md` + skills) must be kept consistent with `docs/` by
  discipline; the linter does not parse them for drift beyond their citation keys.
- IEC 81001-5-1 citations are linted against a provisional index (see `docs/iec81001/README.md`),
  so a "valid" 81001 citation is still only provisionally correct.

## Related

- [ADR-005](ADR-005-pure-rust-project-boundary-and-dependency-policy.md) — trust zones the linter
  and dependency policy sit in
- [ADR-007](ADR-007-compliance-evidence-and-generated-artifact-ownership.md) — the CI-verified
  evidence pattern the linter extends to documentation
- [ADR-019](ADR-019-regulatory-standards-reference-corpus.md) — the corpus this standardizes
  access to
- [`docs/governance/citation-convention.md`](../governance/citation-convention.md)
- [`AGENTS.md`](../../AGENTS.md), [`.claude/skills/`](../../.claude/skills/)
