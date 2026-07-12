# Regulatory compliance

## Purpose and scope

This document explains how MduX-rust's engineering practices are designed to align with IEC
62304 Class B/C software-development expectations, and how the artifacts it generates are meant
to feed a manufacturer's own technical file and notified-body audits.

**This document, and MduX-rust itself, is not a regulatory clearance, not a Quality Management
System, and not a substitute for a manufacturer's own processes.** MduX-rust is engineering
scaffolding: a template, a set of governed crates, and evidence-generation tooling that a
manufacturer integrates into their own ISO 13485 QMS, their own ISO 14971 risk management file,
and their own engagement with a notified body. Nothing in this repository is itself a certified
or cleared medical device.

## The Class B/C pain points this framework targets

Teams building Class B/C medical software under IEC 62304 repeatedly run into the same friction:

- **Requirement-to-verification traceability** is usually bolted on after the fact — spreadsheets
  maintained by hand, drifting from the code they're supposed to describe.
- **Third-party dependency surface** (SOUP — Software of Unknown Provenance) grows fastest in the
  UI and, increasingly, the AI/ML layers of a device — exactly the layers most visible to an
  operator and most likely to need frequent updates.
- **Evidence reproducibility**: an auditor asking "can you reproduce this build and show me it
  matches what shipped" is a routine, and routinely painful, question.
- **Deterministic behavior** of safety-critical UI elements (alarms, dosage displays, status
  indicators) is hard to guarantee when the rendering/localization stack allocates, shapes text,
  or resolves fallback fonts at runtime.

MduX-rust's crate `mdux-governance` (`crates/mdux-governance/src/lib.rs`) provides working types
for the first point directly: `Requirement`, `Hazard`, `VerificationCase`, `ProblemReport`, and an
`AuditEvent` trail, composed by a `ComplianceProgram` that:

- refuses to validate (`ComplianceProgram::validate()`) unless every requirement has at least one
  verification case, and — for Class C — at least one declared hazard;
- exports a structured requirement-to-verification-to-hazard trace matrix
  (`ComplianceProgram::trace_rows()` / `trace_matrix_export()`), the machine-checkable backbone of
  a traceability table;
- exports a sequenced audit trail (`ComplianceProgram::audit_export()`) recording every
  requirement, hazard, verification, and problem report registered against the device.

These are real, tested types you compose in application code (see `examples/hello_world/src/main.rs`
and `examples/class_c_monitor/src/main.rs`) — not aspirational documentation.

## The trust-zone architecture narrows what needs deep review

The workspace splits into three directories that double as a formal trust-zone declaration
([ADR-005](adr/ADR-005-pure-rust-project-boundary-and-dependency-policy.md),
[ADR-012](adr/ADR-012-presentation-adapter-crates-and-shader-artifacts.md); full detail in
[Architecture](architecture.md)):

- **`crates/` (governed)** — pure Rust, `#![forbid(unsafe_code)]`, no FFI types or native handles
  in any public API. This is the small core a reviewer needs to examine line-by-line.
- **`adapters/`** — the only place native Vulkan/windowing bindings are allowed, and only behind a
  boundary that accepts/returns owned data already defined by a governed crate. No foreign type
  crosses back into the governed core.
- **`tools/`** — host-only, offline, never linked into a runtime artifact.

The practical consequence for a notified-body reviewer: instead of auditing the entire dependency
graph uniformly, review effort concentrates on a deliberately small, `unsafe`-free governed core,
while everything in `adapters/`/`tools/` is handled through the SOUP register and generated,
byte-verified evidence described below — a scoping argument, not a claim that those zones are
exempt from scrutiny.

## The evidence-generation pattern maps onto audit expectations

Every asset pipeline in the repo (fonts, images, shaders, ML weights) follows the same shape
([ADR-007](adr/ADR-007-compliance-evidence-and-generated-artifact-ownership.md)): a host-only
`tools/*-baker` binary compiles a reviewed source input plus a recipe into a committed
`package.json` (the data) and `report.json` (a SHA-256 digest, tool version, and the exact options
used). CI runs only `verify`, which re-derives the artifact and checks it is byte-identical to
what's committed — `tools/mdux-font-baker`, `tools/mdux-image-baker`, `tools/mdux-shader-baker`,
`tools/mdux-ml-baker` all work this way.

This is directly what an IEC 62304 §5-8/§9 verification record and configuration-management
process ask for: a documented, reproducible link between a source input and a shipped artifact,
checked automatically rather than asserted by hand.

## Zero-SOUP ML as a concrete differentiator

Machine-learning inference is usually where a Class C device's SOUP exposure grows fastest — an
ONNX Runtime or PyTorch dependency is a large, general-purpose native stack that a manufacturer
did not write and cannot fully audit. ADR-017 takes a different approach for MduX-rust:

- **Weights are data, not code.** `tools/mdux-ml-baker` bakes a `ModelPackage` offline and commits
  it as evidence (`generated/models/<id>/{package.json,report.json}`), byte-verified by CI. The
  demonstrator model that ships in `examples/class_c_monitor` (`generated/models/eeg-demo/`) was
  baked from openly available weights; a manufacturer's own clinically-qualified weights go
  through the identical pipeline. **Swapping the weights changes zero lines of `mdux-ml-runtime`
  or application source.**
- **The inference engine itself (`crates/mdux-ml-runtime`) is from-scratch Rust**: Dense/Conv1D/
  pooling/activation kernels as plain, strictly-ordered scalar loops, `#![forbid(unsafe_code)]`,
  no SIMD, no FMA — no ONNX Runtime, PyTorch, `tract`, or other native/foreign inference stack is
  ever linked into a device crate.
- **`Classifier1D::new()` fails closed.** Every baked package carries host-computed golden
  input/output vectors; construction re-runs every one of them and refuses to proceed if any
  output diverges bit-for-bit. This is a genuine Class C safety control — it catches toolchain
  miscompilation, target floating-point drift, or a corrupted/mismatched package *before* the
  device classifies a real signal, the same way `TextRuntime::new()` validates a text package once
  at startup.

## The SOUP register as a worked example

[`docs/governance/soup-register.toml`](governance/soup-register.toml) is a live, structured
register of every third-party (SOUP) dependency reachable from the host tooling and presentation
adapters — the shape a notified body will want to see for SOUP classification and justification
under IEC 62304 §8. Each entry records, as data (not prose scattered across a wiki):
`component_id`, `supplier`, `repository`, `license`, `usage`, `integration_path`, `pinned_by`
(the exact `Cargo.toml`/`Cargo.lock` files pinning it), whether it reaches `runtime_deployment`,
whether it is a `yocto_target_dependency`, its `support_model`, a `boundary_rationale` explaining
which trust zone confines it, and a list of `risk_controls`. This register is kept current with
the repository as crates move between zones (for example, when `ash`/`winit` moved from
`examples/hello_world` directly into the `adapters/mdux-vulkan-winit` edge adapter) — a register
that drifts from the code it describes is worse than no register at all.

## ADRs as design-rationale input

The [18 accepted ADRs](adr/README.md) are the project's design-history record: why the
governed/adapter/tools boundary exists, why `.medui` is compile-time-only, why ML inference is
built the way it is. Collectively they are the kind of rationale trail a technical file's
design-and-development section draws on — read the index rather than this document re-deriving
each one.

## Governance types are scaffolding, not an operating QMS

`mdux-governance`'s types give an application a place to *record* requirements, hazards,
verifications, and audit events in a structured, exportable form. They do not, by themselves,
constitute an operating quality system: nothing in this repository performs management review,
CAPA, supplier qualification, post-market surveillance, or any of the other ISO 13485 processes a
manufacturer's QMS is responsible for. A manufacturer populates and operates these types as part
of their own process — MduX-rust supplies the data model and the export format, not the process
itself.

## Roadmap: standards references and regulatory document templates

Two efforts are prioritized next, specifically to cover the needs common to the majority of
Class B/C medical-device software rather than just the NeuroSense 500 demonstrator's own
requirements. Neither is shipped yet.

- **Standards references usable by developers' LLMs.** The framework's original C++ project
  (`MduX`) already prototyped this: a markdown version of IEC 62304 broken into modules by
  life-cycle process (`docs/iec62304/`), an "AI Reference" document per standard
  (`MduX-IEC-62304-AI-Reference.md`, `MduX-ISO-13485-AI-Reference.md`), and JSON automation
  schemas for safety classification, traceability, and risk management, explicitly designed to be
  consumed by an AI agent during development rather than only by a human reading the standard
  end to end. MduX-rust will port and adapt this corpus — starting from the existing IEC 62304,
  ISO 13485, and ISO 14971 material, then adding IEC 62366-1 (usability engineering) and
  IEC 81001-5-1 (software life-cycle cybersecurity), neither of which the C++ project covered yet
  — so a developer's AI assistant can cite the exact clause text and generate code or
  documentation aligned with the corresponding requirement. This does not replace a regulatory
  expert's judgment; it gives the assistant a grounded, structured reference instead of relying on
  its own (unverifiable) recollection of a standard's contents.
- **Regulatory documentation templates.** A `software_development_file/regulatory/` tree will
  provide, standard by standard, a document skeleton a manufacturer fills in and adapts to their
  own product instead of starting from a blank page:

  ```text
  software_development_file/
  └── regulatory
      ├── IEC_62304
      │   ├── SAD.md      # Software Architecture Design
      │   ├── SDD.md      # Software Design Description
      │   └── SOUP.md     # SOUP list and justification
      ├── IEC_62366
      │   └── Usability_Engineering_File.md
      ├── IEC_81001
      │   └── Cybersecurity_SAD.md
      ├── ISO_13485
      │   └── README.md
      └── ISO_14971
          └── Risk_Management_File.md
  ```

  Eventually, `ComplianceProgram`'s structured export (`trace_matrix_export`, `audit_export`)
  should be able to feed these documents directly instead of remaining an artifact a manufacturer
  copies in by hand — closing the loop between the governance types described above and the
  paperwork a notified body actually reads.

These templates would still need a manufacturer's own content, review, and sign-off before they
constitute part of a real technical file — see the scope boundary below.

## What this project does and does not provide

> **Does not provide:** an operating ISO 13485 quality management system; a completed ISO 14971
> risk management file; an actual notified-body engagement or submission; clinical evaluation; a
> certified or cleared product; legal or regulatory advice.
>
> **Does provide:** a governed, `unsafe`-free Rust core with a documented trust-zone boundary; a
> reproducible, byte-verified evidence-generation pattern for fonts, images, shaders, and ML
> weights; a zero-SOUP, fail-closed ML inference engine; working requirement/hazard/verification/
> audit-trail types with structured export; and an ADR trail documenting the design rationale
> behind all of it.

Throughout this document and the README, wording is deliberately chosen to say "supports,"
"provides evidence for," or "is designed to align with" — never "guarantees," "ensures," "is
compliant with," or "certified." Treat any stronger claim you find elsewhere in this repository as
a bug worth reporting.

## Where to go next

- [Architecture](architecture.md) — the trust-zone boundary and crate map in full detail.
- [ADR index](adr/README.md) — all 18 accepted architecture decision records.
- [`docs/governance/soup-register.toml`](governance/soup-register.toml) — the SOUP register itself.
- [Getting started](getting-started.md) — full walkthroughs of the example applications.
