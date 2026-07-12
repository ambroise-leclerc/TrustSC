# Software Design Description (SDD)

> Template — IEC 62304 §5.4 (Software detailed design). Fill in every `[ ... ]` placeholder. See
> [`docs/iec62304/03-development-design.md`](../../../docs/iec62304/03-development-design.md) for the
> underlying clause-by-clause guidance. See
> `software_development_file/regulatory/IEC_62304/SDD.md` (added in a later PR in this stack) for
> MduX-rust's own filled-in example.

## Document control

- **Software item(s) covered:** [ ... ]
- **Version:** [ ... ]
- **Author(s):** [ ... ]
- **Date:** [ YYYY-MM-DD ]
- **Approval:** [ ... ]

## 1. Purpose and scope

[ Which software item(s)/units from the SAD does this SDD detail? ]

## 2. Detailed design per software unit

> `IEC 62304:2006 §5.4.1 Refine the software architecture into a detailed design`

For each software unit:

### Unit: [ name ]
- **Responsibility:** [ ... ]
- **Internal structure:** [ modules, types, key algorithms ]
- **Dependencies:** [ other units, SOUP ]

## 3. Interface detailed design

> `IEC 62304:2006 §5.4.2 Develop a detailed design for interfaces`

[ Public function signatures, data formats, error conditions for each interface identified in the
SAD. ]

## 4. Detailed design verification

> `IEC 62304:2006 §5.4.3 Verify the detailed design`

[ How was this detailed design verified — reviews, unit test coverage, static analysis? ]

## Justification records

```json
{
  "justification_id": "JUS-NNN",
  "standard": "IEC 62304",
  "clause_ref": "IEC 62304:2006 §5.4.1 Refine the software architecture into a detailed design",
  "rationale": "[ ... ]",
  "evidence_refs": ["[ ... ]"]
}
```
