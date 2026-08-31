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
lane_seq: 5
collision: [v7-prelude, v7-test]
blocked_by: ['@dl7-structural-conformance']
commits:
- hash: c16546295
  summary: Prove DL7 userland type algebra
closed: 2026-08-31
---

# Constrain a generic with conformance

## Description

## Description

Implement one generic whose rule body requires Conforms before interning its result.

## Acceptance Criteria

- [x] A conforming source constructs the generic result.
- [x] A failing source derives no result.
- [x] No generic-only checker form is added.

## Tests Run

- [x] Positive and negative generic constraint receipts pass.
