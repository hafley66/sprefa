---
created: 2026-08-31
updated: 2026-08-31
type: task
status: open
priority: high
epic: dl7-type-algebra
labels:
- size:med
size: M
lane: dl7-type-algebra
lane_seq: 10
collision: [v7-prelude, v7-test]
blocked_by: ['@dl7-intersection-edges']
---

# Compose a userland type operator

## Description

## Description

Implement one additional userland type composition relation by calling Intersect in an ordinary rule body.

## Acceptance Criteria

- [ ] Composition leaves no bespoke compiler implementation.
- [ ] Nested expression application reaches the canonical result.

## Tests Run

- [ ] Nested composition receipt passes.
