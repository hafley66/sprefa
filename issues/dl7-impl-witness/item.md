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
lane_seq: 8
collision: [v7-prelude, v7-test]
blocked_by: ['@dl7-structural-conformance', '@dl7-relation-contract']
commits:
- hash: c16546295
  summary: Prove DL7 userland type algebra
closed: 2026-08-31
---

# Validate explicit impl witnesses

## Description

## Description

Treat implements(Contract, Source, Witness) as authored evidence and derive valid_impl or invalid_impl_edge through conformance.

## Acceptance Criteria

- [x] Valid witnesses derive proof rows.
- [x] Invalid witnesses retain failed edge data.
- [x] No impl syntax form is added.

## Tests Run

- [x] Valid and invalid witness receipts pass.
