# Risk Management File

> Template — ISO 14971:2019. Fill in every `[ ... ]` placeholder. See
> [`docs/iso14971/README.md`](../../../docs/iso14971/README.md) for the underlying clause-by-clause
> guidance, and [`docs/iec62304/06-risk-management-process.md`](../../../docs/iec62304/06-risk-management-process.md)
> for the software-specific slice of this process. See
> [`software_development_file/regulatory/ISO_14971/Risk_Management_File.md`](../../regulatory/ISO_14971/Risk_Management_File.md)
> for MduX-rust's own filled-in example.

## Document control

- **Product / software item:** [ ... ]
- **Version:** [ ... ]
- **Author(s):** [ ... ]
- **Date:** [ YYYY-MM-DD ]
- **Approval:** [ ... ]

## 1. Risk management plan summary

> `§4 General requirements for risk management system`

[ Scope, criteria for risk acceptability, who's responsible. ]

## 2. Risk analysis

> `§5`

[ Intended use/misuse, identified hazards and hazardous situations, estimated risk per hazard —
reference individual risk records, e.g. using
[`docs/iso14971/schemas/risk-record.schema.json`](../../../docs/iso14971/schemas/risk-record.schema.json). ]

## 3. Risk evaluation

> `§6`

[ Which estimated risks are acceptable as-is, and which require control. ]

## 4. Risk control

> `§7`

[ Risk control measures selected, implemented (cross-reference software Requirements where the
control is a software measure), and verified. ]

## 5. Overall residual risk evaluation

> `§8`

[ Is overall residual risk acceptable, considering all controls together? ]

## 6. Risk management review

> `§9`

[ Confirms the plan was executed, residual risks are acceptable, and monitoring for production/
post-production information is in place. ]

## 7. Production and post-production activities

> `§10`

[ How field information (complaints, incidents, near-misses) feeds back into this file. ]

## Justification records

```json
{
  "justification_id": "JUS-NNN",
  "standard": "ISO 14971",
  "clause_ref": "ISO 14971:2019 §7.1 Risk reduction",
  "rationale": "[ ... ]",
  "requirement_id": "[ optional mdux-governance RequirementId ]",
  "evidence_refs": ["[ ... ]"]
}
```
