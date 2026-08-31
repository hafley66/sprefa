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
lane_seq: 0
collision: [v7-lowerer, v7-test]
closed: 2026-08-30
closed_by: codex-0
commits:
- hash: 0c38a71c8
  summary: Add DL7 expression result carrier
---

# Add DL7 expression result carrier

## Description

Introduce the internal Value plus Goals plus Origins carrier and focused lowering tests for atomic expression values. Model class: Luna.

## Acceptance Criteria

- [x] One internal carrier returns a value, ordered generated goals, and origins.
- [x] Atoms, literals, and variables have exact focused receipts.
- [x] Existing explicit call lowering is unchanged.

## Tests Run

- [x] Focused V7 SWI tests pass.

## Implementation Notes

Plan milestone 1. Do not add a new production file unless the current lowerer
crosses the repository hard size boundary.

## Resolution

### 2026-08-30T21:18:18Z · @codex-0

Carrier landed in the DL7 feature worktree. The focused dl7_entrypoints gate passed 12 of 12 in 1.4 seconds.
