---
created: 2026-08-31
updated: 2026-08-31
type: task
status: done
priority: high
epic: dl7-type-algebra
labels:
- size:med
size: M
lane: dl7-type-algebra
lane_seq: 10
collision: [v7-prelude, v7-test]
blocked_by: ['@dl7-intersection-edges']
commits:
- hash: c16546295
  summary: Prove DL7 userland type algebra
closed: 2026-08-31
---

# Compose a userland type operator

## Description

## Description

Implement one additional userland type composition relation by calling Intersect in an ordinary rule body.

## Acceptance Criteria

- [x] Composition leaves no bespoke compiler implementation.
- [x] Nested expression application reaches the canonical result.

## Tests Run

- [x] Nested composition receipt passes.
