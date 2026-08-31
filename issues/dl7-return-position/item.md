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
lane_seq: 1
collision: [v7-lowerer, v7-checker]
blocked_by: ['@dl7-expression-carrier']
closed: 2026-08-30
closed_by: codex-0
commits:
- hash: b01046b8b
  summary: Resolve DL7 expression return positions
---

# Resolve declared expression return positions

## Description

Resolve exactly one return-labeled edge for expression use while leaving explicit full-tuple calls unchanged. Model class: Terra.

## Acceptance Criteria

- [x] Expression use resolves one declared `return` edge and its position.
- [x] Zero and multiple return edges produce positioned diagnostics.
- [x] Full-arity relation calls require no return edge.

## Tests Run

- [x] Focused V7 SWI tests pass.

## Implementation Notes

Plan milestone 2. Return is projection metadata over an ordinary tuple rather
than an evaluator direction.

## Resolution

### 2026-08-30T21:19:32Z · @codex-0

Return projection lookup landed in the DL7 feature worktree. The focused dl7_entrypoints gate passed 13 of 13 in 1.4 seconds.
