---
created: 2026-08-30
updated: 2026-08-30
type: task
status: done
priority: high
epic: dl7-expression-flow
labels: [compiler]
size: L
lane: dl7-expression-flow
lane_seq: 9
collision: [v7-lowerer, v7-checker, v7-prelude, v7-test]
blocked_by: ['@dl7-partial-application']
closed: 2026-08-30
commits:
- hash: ff9884516
  summary: Prove compound DL7 key labels
---

# Prove compound edge labels with Key

## Description

Permit ground compound labels on ordered type edges and prove a userland Key constructor with typed options and composite-key collection. Model class: Direct high.

## Acceptance Criteria

- [x] Ordered type edges accept a ground compound label value.
- [x] Lexical name resolution remains defined for atom labels.
- [x] Key options remain attached to the literal edge value.
- [x] Closing an owner derives ordered composite-key rows from all keyed edges.

## Tests Run

- [x] Compound-label and composite-key focused tests pass.
- [x] Complete V7 SWI and Tree-sitter gates pass.

## Implementation Notes

Plan milestone 10. Stop for a user ruling if options participating in edge
identity cannot be separated from projection spelling without choosing new
surface syntax.

## Resolution

### 2026-08-31T00:05:27Z · @issuectl

Ground compound labels, preserved Key options, dense composite-key rows, and complete V7 gates are verified.
