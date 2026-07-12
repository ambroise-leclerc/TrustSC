# Cybersecurity Software Architecture Design

> Template — IEC 81001-5-1:2021. Fill in every `[ ... ]` placeholder. See
> [`docs/iec81001/README.md`](../../../docs/iec81001/README.md) for the underlying guidance — note
> that document's caveat about clause-numbering uncertainty for this newer standard. See
> `software_development_file/regulatory/IEC_81001/Cybersecurity_SAD.md` (added in a later PR in this stack)
> for MduX-rust's own filled-in example.

## Document control

- **Product / software item:** [ ... ]
- **Version:** [ ... ]
- **Author(s):** [ ... ]
- **Date:** [ YYYY-MM-DD ]
- **Approval:** [ ... ]

## 1. Scope and relationship to the IEC 62304 lifecycle

> `IEC 81001-5-1:2021 §4-§5 (approx.) Scope and relationship to IEC 62304's life cycle processes`

[ How does security risk management integrate with this product's IEC 62304 process? Reference the
product's SAD (`software_development_file/.../IEC_62304/SAD.md`). ]

## 2. Security risk management

> `IEC 81001-5-1:2021 §5 (approx.) Security risk management`

[ Threats and vulnerabilities identified, their assessed risk, and the security controls applied.
Cross-reference safety hazards from the ISO 14971 risk management file where a security issue could
also cause safety harm. ]

## 3. Secure design and implementation

> `IEC 81001-5-1:2021 §6 (approx.) Secure design and secure implementation`

[ Secure-design principles applied — trust boundaries, privilege segregation, memory-safety
guarantees, dependency review policy. ]

## 4. Security verification

> `IEC 81001-5-1:2021 §6 (approx.) Security verification`

[ How security controls were verified — testing, static analysis, dependency scanning. ]

## 5. Security update management

> `IEC 81001-5-1:2021 §7 (approx.) Security update management`

[ How security updates reach a fielded device. If the product has no network connectivity or
update mechanism, state that explicitly rather than leaving this section silently blank. ]

## 6. Security guidelines for users

> `IEC 81001-5-1:2021 §7 (approx.) Security guidelines for users`

[ What security-relevant information/guidance is provided to the device's operators/IT
administrators? ]

Every `§... (approx.)` reference above is a provisional section placeholder, per the caveat in
[`docs/iec81001/README.md`](../../../docs/iec81001/README.md) — check it against your own licensed
copy of the current edition before citing it in a real submission.

## Justification records

```json
{
  "justification_id": "JUS-NNN",
  "standard": "IEC 81001-5-1",
  "clause_ref": "IEC 81001-5-1:2021 §5 (approx.) Security risk management",
  "rationale": "[ ... ]",
  "evidence_refs": ["[ ... ]"]
}
```
