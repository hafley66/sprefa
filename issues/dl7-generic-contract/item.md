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
lane_seq: 5
collision: [v7-prelude, v7-test]
blocked_by: ['@dl7-structural-conformance']
---

# Constrain a generic with conformance

## Description

## Description

Implement one generic whose rule body requires Conforms before interning its result.

## Acceptance Criteria

- [ ] A conforming source constructs the generic result.
- [ ] A failing source derives no result.
- [ ] No generic-only checker form is added.

## Tests Run

- [ ] Positive and negative generic constraint receipts pass.
