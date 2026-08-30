---
created: 2026-08-30
updated: 2026-08-30
type: task
status: in-progress
priority: high
epic: dl7-expression-flow
labels: [compiler]
size: M
lane: dl7-expression-flow
lane_seq: 1
collision: [v7-lowerer, v7-checker]
blocked_by: ['@dl7-expression-carrier']
---

# Resolve declared expression return positions

## Description

Resolve exactly one return-labeled edge for expression use while leaving explicit full-tuple calls unchanged. Model class: Terra.

## Acceptance Criteria

- [ ] Expression use resolves one declared `return` edge and its position.
- [ ] Zero and multiple return edges produce positioned diagnostics.
- [ ] Full-arity relation calls require no return edge.

## Tests Run

- [ ] Focused V7 SWI tests pass.

## Implementation Notes

Plan milestone 2. Return is projection metadata over an ordinary tuple rather
than an evaluator direction.
