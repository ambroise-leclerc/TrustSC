# Risk Management File

> Template — ISO 14971:2019. Fill in every `[ ... ]` placeholder. See
> [`docs/iso14971/README.md`](../../../docs/iso14971/README.md) for the underlying clause-by-clause
> guidance, and [`docs/iec62304/06-risk-management-process.md`](../../../docs/iec62304/06-risk-management-process.md)
> for the software-specific slice of this process. See
> `software_development_file/regulatory/ISO_14971/Risk_Management_File.md` (added in a later PR in this stack)
> for TrustSC's own filled-in example.

## Document control

- **Product / software item:** [ ... ]
- **Version:** [ ... ]
- **Author(s):** [ ... ]
- **Date:** [ YYYY-MM-DD ]
- **Approval:** [ ... ]

## 1. Risk management plan summary

> `ISO 14971:2019 §4 General requirements for risk management system`

[ Scope, criteria for risk acceptability, who's responsible. ]

## 2. Risk analysis

> `ISO 14971:2019 §5 Risk analysis`

[ Intended use/misuse, identified hazards and hazardous situations, estimated risk per hazard —
reference individual risk records, e.g. using
[`docs/iso14971/schemas/risk-record.schema.json`](../../../docs/iso14971/schemas/risk-record.schema.json). ]

## 3. Risk evaluation

> `ISO 14971:2019 §6 Risk evaluation`

[ Which estimated risks are acceptable as-is, and which require control. ]

## 4. Risk control

> `ISO 14971:2019 §7 Risk control`

[ Risk control measures selected, implemented (cross-reference software Requirements where the
control is a software measure), and verified. ]

## 5. Overall residual risk evaluation

> `ISO 14971:2019 §8 Evaluation of overall residual risk`

[ Is overall residual risk acceptable, considering all controls together? ]

## 6. Risk management review

> `ISO 14971:2019 §9 Risk management review`

[ Confirms the plan was executed, residual risks are acceptable, and monitoring for production/
post-production information is in place. ]

## 7. Production and post-production activities

> `ISO 14971:2019 §10 Production and post-production activities`

[ How field information (complaints, incidents, near-misses) feeds back into this file. ]

## Justification records

```json
{
  "justification_id": "JUS-NNN",
  "standard": "ISO 14971",
  "clause_ref": "ISO 14971:2019 §7.1 Risk reduction",
  "rationale": "[ ... ]",
  "requirement_id": "[ optional trustsc-governance RequirementId ]",
  "evidence_refs": ["[ ... ]"]
}
```
