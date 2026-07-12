# Usability Engineering File

> Template — IEC 62366-1:2015+AMD1:2020. Fill in every `[ ... ]` placeholder. See
> [`docs/iec62366/README.md`](../../../docs/iec62366/README.md) for the underlying clause-by-clause
> guidance. See
> [`software_development_file/regulatory/IEC_62366/Usability_Engineering_File.md`](../../regulatory/IEC_62366/Usability_Engineering_File.md)
> for MduX-rust's own filled-in example.

## Document control

- **Product / software item:** [ ... ]
- **Version:** [ ... ]
- **Author(s):** [ ... ]
- **Date:** [ YYYY-MM-DD ]
- **Approval:** [ ... ]

## 1. Use specification

> `§5.1`

[ Intended medical indication, patient population, intended user profile(s), intended use
environment, operating principle. ]

## 2. Frequently used functions and hazard-related use scenarios

> `§5.2`

[ List of use scenarios, marking which are hazard-related. ]

## 3. User interface specification

> `§5.3`

[ How the UI is specified — reference screen designs, DSL source files, or a design system if one is
used. ]

## 4. User interface evaluation plan

> `§5.4`

[ Plan for formative and summative evaluation — methods, participants, acceptance criteria. ]

## 5. User interface design and implementation

> `§5.5`

[ Reference to the actual implementation. ]

## 6. Formative evaluation

> `§5.6`

[ Results of formative (iterative, developmental) evaluation activities. ]

## 7. Summative evaluation

> `§5.7`

[ Results of summative (validation) evaluation — required for hazard-related use scenarios. ]

## Justification records

```json
{
  "justification_id": "JUS-NNN",
  "standard": "IEC 62366-1",
  "clause_ref": "IEC 62366-1:2015 §5.2 Identify frequently used functions and hazard-related use scenarios",
  "rationale": "[ ... ]",
  "evidence_refs": ["[ ... ]"]
}
```
