---
created: 2026-08-20
updated: 2026-08-21
type: epic
owner: codex
status: open
priority: high
labels:
- area:dl6
- intent:type-system
related: ['@comptime-type-model']
---

# Type surface gaps

## Description

Plan: v6/plans/2026-08-20-type-surface-gaps.md. Close the authored reflection, recursive inline-arrow, and native JSON literal gaps over the canonical type graph.

## Acceptance Criteria

- [ ] Authored DL6 rules can query the canonical type graph.
- [ ] Inline arrow relations work in every recursive type-expression position.
- [ ] Copy-pasted JSON objects and arrays are first-class JSON values.
- [ ] The exhaustive language fixture uses all three surfaces.
- [ ] Complete Prolog compiler CI passes.

## Tests Run

Pending.

## Implementation Notes

Reflection precedes the two syntax cards. Inline-arrow and JSON work may run
independently after reflection CI passes.
