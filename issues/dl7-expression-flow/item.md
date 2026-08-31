---
created: 2026-08-30
updated: 2026-08-30
type: epic
owner: chris
status: done
priority: high
labels: [compiler]
closed: 2026-08-30
commits:
- hash: ff9884516
  summary: Complete ten-stage DL7 expression flow
- hash: e69b34ed8
  summary: Close final child task
---

# DL7 relational expression flow

## Description

Track the ten milestones in plans/2026-08-30-dl7-relational-expression-flow.md: uniform relation expressions, preserved full-tuple reverse queries, application-driven generic construction, compile-known partial application, and compound edge labels.

## Acceptance Criteria

- [x] All ten child milestones close with commit receipts.
- [x] Expression syntax lowers to ordinary checked relation goals.
- [x] Explicit full-tuple calls retain forward, reverse, and mixed-mode use.
- [x] Generic construction has no user-authored trigger relation.
- [x] Compile-known partial applications leave no dynamic runtime apply goal.
- [x] Compound edge labels support a userland Key proof.

## Tests Run

- [x] Complete V7 SWI suite passes.
- [x] Consolidated Tree-sitter corpus passes.

## Implementation Notes

Base implementation branch: `feature/dl7-count-aggregate` at plan commit
`94a921d95`. Child tasks are intentionally serialized through one collision
lane until the expression carrier and return contract stabilize.

## Resolution

### 2026-08-31T00:06:26Z · @issuectl

All ten serialized child tasks are done. SWI passed 23 of 23 and the consolidated Tree-sitter corpus passed 1 of 1.
