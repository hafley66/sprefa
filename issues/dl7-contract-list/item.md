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
lane_seq: 3
collision: [v7-prelude, v7-test]
blocked_by: ['@dl7-structural-conformance']
---

# Check closed contract lists

## Description

## Description

Implement ConformsAll over a closed cons list so one source can satisfy an interface intersection by conjunction.

## Acceptance Criteria

- [ ] Empty and non-empty contract lists terminate.
- [ ] Every listed contract must prove.

## Tests Run

- [ ] Two-contract and failing-list receipts pass.
