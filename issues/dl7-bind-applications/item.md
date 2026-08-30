---
created: 2026-08-30
updated: 2026-08-30
type: task
status: open
priority: high
epic: dl7-expression-flow
labels: [compiler]
size: M
lane: dl7-expression-flow
lane_seq: 2
collision: [v7-lowerer, v7-test]
blocked_by: ['@dl7-return-position']
---

# Lower applications on bind targets

## Description

Lower a relation application on the right side of colon into an ordinary body goal whose result becomes the derived edge target. Model class: Luna.

## Acceptance Criteria

- [ ] `(: UserPatch (Partial User))` lowers without a trigger fact.
- [ ] The generated body contains `Partial(User, Result)`.
- [ ] The generated `:/4` head targets the same result variable.

## Tests Run

- [ ] Focused V7 SWI tests pass.

## Implementation Notes

Plan milestone 3. Preserve source origins for both the call and derived edge.
