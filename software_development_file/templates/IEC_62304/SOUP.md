# SOUP list and justification

> Template — IEC 62304 §5.3.4 / §8.1.3 (SOUP identification and configuration identification) and
> §8.3.2 (SOUP anomaly list, AMD1:2015). See
> [`docs/iec62304/03-development-design.md`](../../../docs/iec62304/03-development-design.md) and
> [`docs/iec62304/07-configuration-management-process.md`](../../../docs/iec62304/07-configuration-management-process.md).
> See [`software_development_file/regulatory/IEC_62304/SOUP.md`](../../regulatory/IEC_62304/SOUP.md)
> for MduX-rust's own filled-in example, which derives from
> [`docs/governance/soup-register.toml`](../../../docs/governance/soup-register.toml).

## Document control

- **Product / software item:** [ ... ]
- **Version:** [ ... ]
- **Author(s):** [ ... ]
- **Date:** [ YYYY-MM-DD ]

## 1. Purpose

[ State that this document lists every SOUP item used by the product, its justification for use, and
the risk controls applied because of it. ]

## 2. SOUP register

For each SOUP item (one entry per dependency; a machine-readable register such as a TOML/JSON file is
recommended over a hand-maintained table for anything beyond a handful of entries):

- **Component:** [ name + version ]
- **Supplier:** [ ... ]
- **License:** [ ... ]
- **Integration path:** [ which software item(s) use it ]
- **Justification for use:** [ why this dependency instead of writing it in-house ]
- **Known anomalies (§8.3.2):** [ tracked defects/CVEs the manufacturer is aware of but did not
  introduce ]
- **Risk controls:** [ how the SOUP item's confinement/usage mitigates the risk of its defects ]

## 3. SOUP update policy

[ How is a new SOUP version evaluated before being adopted? Who approves it? ]

## Justification records

```json
{
  "justification_id": "JUS-NNN",
  "standard": "IEC 62304",
  "clause_ref": "IEC 62304:2006 §5.3.4 Identify SOUP items",
  "rationale": "[ ... ]",
  "evidence_refs": ["[ ... ]"]
}
```
