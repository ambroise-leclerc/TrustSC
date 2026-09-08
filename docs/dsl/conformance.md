# Shared MedUI conformance baseline

TrustSC adopts MedUI `v0.3.0-rc.1` at
`9a57f6462b6f8dbdf1f0b8b4519674f1c6235dbb`, the revision pinned by MduX. The exact commit in
[`medui-conformance.toml`](../../medui-conformance.toml) controls both CI's checkout and the test
harness. This adoption is tracked by [MduX #335](https://github.com/ambroise-leclerc/MduX/issues/335).

## Declared coverage

| Capability | TrustSC gate | Boundary |
|---|---|---|
| Syntax | All five pinned syntax cases, through `parse_medui_bytes` and the production parser | Expectations come from the pinned `case.json` files, including rejection codes and lines. |
| Diagnostic positions | `line-only` | Columns remain absent. A reported column fails this declaration rather than silently claiming full precision. |
| Semantics, layout, safety | Unclaimed | Local compilation exists, but the shared harness has no complete adapters for these phases' inputs and normalized observations. Claiming one fails the gate. |
| RENDERED, EVIDENCE | Unclaimed | Native verification checks/reports are not complete implementations of these observation profiles. No profile or pixel-equivalence claim follows from the syntax pass. |

The new pin adds three syntax cases to the two previously checked. Duplicate authored IDs now fail
while parsing, across root components, Rows and their children; the compiled-node check remains
necessary for synthesized panel IDs and caller-edited ASTs. Invalid UTF-8 now produces `MEDUI-E004`
at its source line, through the public byte parser, file compiler and checker. I/O failures remain
`MEDUI-E003`. The parser still carries no columns.

The harness reports executed and unclaimed cases separately: five of the 27 compiler cases execute.
It fails if the checkout is absent in CI, its revision differs from the manifest, a claimed phase
has no adapter or no cases, or an observation disagrees with the corpus. Regression tests also
exercise a deliberately inverted corpus expectation and an unsupported phase claim. CI exposes
this gate as a named step before the broader test suite.

## Reproduction and review

Check out the exact MedUI commit above, then run from TrustSC:

```sh
export MEDUI_CONFORMANCE_DIR=/path/to/pinned/MedUI
CI=1 cargo test --locked -p trustsc-ui-dsl-authoring --test shared_conformance -- --nocapture
cargo test --locked -p trustsc-ui-dsl-authoring -p trustsc-medui-check
```

Paired consumer heads and results belong in
[MduX's results table](https://github.com/ambroise-leclerc/MduX/blob/develop/docs/roadmap.md#314-per-capability-conformance-results).
A shared pin permits a shared syntax baseline at line precision; it does not substantiate the
unclaimed phases or profiles. MduX retains all four phases, full positions and both profiles.
Joint maintainer review of the paired results is still required before MduX #335 is signed off.

Impact is potentially safety-relevant: earlier duplicate rejection and distinct encoding diagnostics
improve compiler conformance evidence without changing device rendering. The affected interfaces
are the host parser, file compiler and checker; their syntax and CLI tests supply the evidence.
No baked artifacts or device risk-control implementation changes here, and no certification or
production-readiness claim is made.
