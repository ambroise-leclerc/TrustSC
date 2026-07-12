# ADR-006: Vulkan versus Vulkan SC profile strategy

## Status

Accepted

## Context

`TrustSC` needs one framework architecture that can support Class B devices on general Vulkan platforms and Class C devices on Vulkan SC-oriented targets. The workspace already exposes both profiles through `trustsc-ui`, includes separate Class B and Class C examples, and enforces that Class C framework builds select the Vulkan SC profile.

The open question is whether Vulkan and Vulkan SC should become separate framework variants or remain two profiles within the same product architecture.

## Decision

- Keep a single UI framework API and treat **Vulkan** and **Vulkan SC** as graphics profiles selected by configuration, not as separate frameworks.
- Use Vulkan as the host bring-up and Class B profile, where dynamic pipeline creation and broader developer hardware support are acceptable.
- Use Vulkan SC as the constrained deployment profile for Class C systems and for any release claim that depends on offline-validated pipelines, zero runtime allocation, and explicit resource budgets.
- Define shared UI, text, and compliance contracts against the stricter common subset so that mandatory runtime behavior remains compatible with Vulkan SC constraints.
- Stamp rendering-affecting generated assets and release evidence with their target profile so that validation records distinguish Vulkan artifacts from Vulkan SC artifacts.

## Consequences

- The workspace can serve Class B and Class C products without forking the framework surface.
- Features that cannot operate within Vulkan SC rules cannot become required behavior of the core runtime path.
- Teams may validate logic and ergonomics on general Vulkan platforms, but Class C acceptance still requires Vulkan SC-specific assets and evidence.
