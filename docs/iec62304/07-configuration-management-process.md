# IEC 62304: Software configuration management process

## Module overview

§8 requires a manufacturer to be able to identify exactly what software configuration was built,
control changes to it, and account for its status at any point — the traceable-build backbone every
other process clause assumes exists.

**Key areas covered:**
- Configuration identification
- Change control
- Configuration status accounting, including SOUP and its known anomalies

---

## §8.1 Configuration identification

### §8.1.1-§8.1.2 Establish and use a configuration identification scheme

A configuration item is anything separately identified and version-controlled: a crate, a generated
evidence artifact, a vendored asset. TrustSC's workspace `Cargo.toml`/`Cargo.lock` pair identifies
every Rust dependency's exact resolved version; `Cargo.lock` is committed and CI builds with
`--locked` (see `docs/architecture.md`'s "Continuous integration" section), so "what was built" is
never ambiguous between a developer's machine and CI.

### §8.1.3 SOUP identification

Every SOUP item gets its own configuration identity in `docs/governance/soup-register.toml`:
`component_id` (a name-version slug), `name`, `version`, `supplier`, `repository`, `license`, and
`pinned_by` (which `Cargo.toml`/`Cargo.lock` paths actually pin that version) — see module 03
(§5.3.4) for where SOUP identification happens at the architecture level.

## §8.2 Change control

### §8.2.1-§8.2.3 Approve, implement, and document changes

Generated evidence artifacts (fonts, shaders, ML model packages) are change-controlled by
byte-verified digest, not by trusting that a regeneration reproduces the same bytes: each `bake`
step commits a `package.json` + `report.json` pair, and `verify` re-derives the digest and fails on
mismatch (ADR-007). This makes an unreviewed or accidental change to a generated artifact fail CI
rather than merge silently — configuration status accounting (§8.3) for these artifacts is therefore
partly automatic rather than fully manual.

## §8.3 Configuration status accounting

### §8.3.1 Maintain records of configuration item history

`trustsc_governance::AuditEvent`'s `Lifecycle` category records every `add_requirement`/`add_hazard`/
`add_problem_report` call with a sequence number (`ComplianceProgram::record_event`), giving the
governance-data half of configuration status a queryable history via `audit_events()`/`audit_export()`.

### §8.3.2 SOUP anomaly list

AMD1:2015 added an expectation that a manufacturer track known SOUP anomalies (defects in a SOUP
component the manufacturer knows about but did not introduce) as part of configuration status
accounting. `docs/governance/soup-register.toml`'s `risk_controls` field per entry is the closest
existing mechanism to this today — it records controls applied *because of* the SOUP dependency's
trust-zone confinement, but does not yet track upstream-reported anomalies/CVEs as a distinct list.
This is a gap for a manufacturer to close in their own configuration management plan, not something
TrustSC currently automates; see `docs/iec62304/08-problem-resolution-process.md` for the related
problem-resolution-process expectations.

---

## Related documents

- [Development design](03-development-design.md)
- [Maintenance process](05-maintenance-process.md)
- [Problem resolution process](08-problem-resolution-process.md)
- [SOUP register](../governance/soup-register.toml)
- [AI-Reference index](AI-Reference.md)
- [Citation convention](../governance/citation-convention.md)
