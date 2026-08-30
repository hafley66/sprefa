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
lane_seq: 0
collision: [v7-lowerer, v7-test]
---

# Add DL7 expression result carrier

## Description

Introduce the internal Value plus Goals plus Origins carrier and focused lowering tests for atomic expression values. Model class: Luna.

## Acceptance Criteria

- [ ] One internal carrier returns a value, ordered generated goals, and origins.
- [ ] Atoms, literals, and variables have exact focused receipts.
- [ ] Existing explicit call lowering is unchanged.

## Tests Run

- [ ] Focused V7 SWI tests pass.

## Implementation Notes

Plan milestone 1. Do not add a new production file unless the current lowerer
crosses the repository hard size boundary.
