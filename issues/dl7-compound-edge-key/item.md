---
created: 2026-08-30
updated: 2026-08-30
type: task
status: open
priority: high
epic: dl7-expression-flow
labels: [compiler]
size: L
lane: dl7-expression-flow
lane_seq: 9
collision: [v7-lowerer, v7-checker, v7-prelude, v7-test]
blocked_by: ['@dl7-partial-application']
---

# Prove compound edge labels with Key

## Description

Permit ground compound labels on ordered type edges and prove a userland Key constructor with typed options and composite-key collection. Model class: Direct high.

## Acceptance Criteria

- [ ] Ordered type edges accept a ground compound label value.
- [ ] Lexical name resolution remains defined for atom labels.
- [ ] Key options remain attached to the literal edge value.
- [ ] Closing an owner derives ordered composite-key rows from all keyed edges.

## Tests Run

- [ ] Compound-label and composite-key focused tests pass.
- [ ] Complete V7 SWI and Tree-sitter gates pass.

## Implementation Notes

Plan milestone 10. Stop for a user ruling if options participating in edge
identity cannot be separated from projection spelling without choosing new
surface syntax.
