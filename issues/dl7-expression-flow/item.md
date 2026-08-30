---
created: 2026-08-30
updated: 2026-08-30
type: epic
owner: chris
status: open
priority: high
labels: [compiler]
---

# DL7 relational expression flow

## Description

Track the ten milestones in plans/2026-08-30-dl7-relational-expression-flow.md: uniform relation expressions, preserved full-tuple reverse queries, application-driven generic construction, compile-known partial application, and compound edge labels.

## Acceptance Criteria

- [ ] All ten child milestones close with commit receipts.
- [ ] Expression syntax lowers to ordinary checked relation goals.
- [ ] Explicit full-tuple calls retain forward, reverse, and mixed-mode use.
- [ ] Generic construction has no user-authored trigger relation.
- [ ] Compile-known partial applications leave no dynamic runtime apply goal.
- [ ] Compound edge labels support a userland Key proof.

## Tests Run

- [ ] Complete V7 SWI suite passes.
- [ ] Consolidated Tree-sitter corpus passes.

## Implementation Notes

Base implementation branch: `feature/dl7-count-aggregate` at plan commit
`94a921d95`. Child tasks are intentionally serialized through one collision
lane until the expression carrier and return contract stabilize.
