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
lane_seq: 11
collision: [v7-test]
blocked_by: ['@dl7-contract-list', '@dl7-generic-contract', '@dl7-impl-witness', '@dl7-history-contract', '@dl7-type-compose']
commits:
- hash: c16546295
  summary: Prove DL7 userland type algebra
closed: 2026-08-31
---

# Pin the type algebra oracle

## Description

## Description

Add consolidated deterministic PLUnit snapshots for conformance, lists, constraints, intersection, impl evidence, and HistoryV1.

## Acceptance Criteria

- [x] One fixture covers the complete type-algebra slice.
- [x] Assertions use exact normalized rows and identities.
- [x] Existing V7 coverage remains active.

## Tests Run

- [x] Complete SWI suite passes.
- [x] Tree-sitter corpus passes.
