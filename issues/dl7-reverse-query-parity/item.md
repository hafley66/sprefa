---
created: 2026-08-30
updated: 2026-08-30
type: task
status: in-progress
priority: high
epic: dl7-expression-flow
labels: [compiler]
size: S
lane: dl7-expression-flow
lane_seq: 6
collision: [v7-test]
blocked_by: ['@dl7-trigger-removal']
---

# Prove full-tuple reverse query parity

## Description

Add exact tests showing a known generic result can bind its source through the unchanged full relation tuple. Model class: Flash4.

## Acceptance Criteria

- [ ] One explicit full-tuple query binds source from known result.
- [ ] The query uses the same constructor relation as forward expression use.
- [ ] Expression lowering does not rewrite explicit full-arity calls.

## Tests Run

- [ ] Focused V7 SWI test passes.

## Implementation Notes

Plan milestone 7. This is the executable receipt for retained Prolog symmetry.
