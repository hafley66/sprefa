---
created: 2026-08-30
updated: 2026-08-30
type: task
status: done
priority: high
epic: dl7-expression-flow
labels: [compiler]
size: M
lane: dl7-expression-flow
lane_seq: 2
collision: [v7-lowerer, v7-test]
blocked_by: ['@dl7-return-position']
closed: 2026-08-30
closed_by: codex-0
commits:
- hash: 504aed475
  summary: Lower DL7 relation expressions in binds
- hash: 5d79f6f73
  summary: Drive Partial from relation application
---

# Lower applications on bind targets

## Description

Lower a relation application on the right side of colon into an ordinary body goal whose result becomes the derived edge target. Model class: Luna.

## Acceptance Criteria

- [x] `(: UserPatch (Partial User))` lowers without a trigger fact.
- [x] The generated body contains `Partial(User, Result)`.
- [x] The generated `:/4` head targets the same result variable.

## Tests Run

- [x] Focused V7 SWI tests pass.

## Implementation Notes

Plan milestone 3. Preserve source origins for both the call and derived edge.

## Resolution

### 2026-08-30T21:34:45Z · @codex-0

Computed bind rules and the trigger-free UserPatch proof landed. Complete V7 SWI passed 19 of 19.
