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
lane_seq: 9
collision: [v7-prelude, v7-test]
blocked_by: ['@dl7-structural-conformance']
commits:
- hash: c16546295
  summary: Prove DL7 userland type algebra
closed: 2026-08-31
---

# Require HistoryV1 source contracts

## Description

## Description

Read a contract type from HistoryV1 options and require Conforms before type and runtime generation.

## Acceptance Criteria

- [x] Existing copy behavior survives with a valid contract.
- [x] An invalid source does not generate history relation or rule rows.

## Tests Run

- [x] Valid and invalid HistoryV1 receipts pass.
