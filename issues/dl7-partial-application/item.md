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
lane_seq: 8
collision: [v7-lowerer, v7-compiler, v7-test]
blocked_by: ['@dl7-expression-modes']
---

# Erase compile-known partial applications

## Description

Intern unsaturated compile-known calls, append later arguments, and erase the partial carrier into a direct first-order call before checked runtime Datalog. Model class: Direct high.

## Acceptance Criteria

- [ ] Callable plus ordered bound arguments determines one partial identity.
- [ ] Later application appends arguments in declared order.
- [ ] Checked runtime rules contain only direct first-order relation calls.

## Tests Run

- [ ] Curried generic focused test passes.
- [ ] Complete V7 SWI and Tree-sitter gates pass.

## Implementation Notes

Plan milestone 9. Dynamic higher-order runtime dispatch remains outside this
slice.
