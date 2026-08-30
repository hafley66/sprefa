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
lane_seq: 8
collision: [v7-lowerer, v7-compiler, v7-test]
blocked_by: ['@dl7-expression-modes']
closed: 2026-08-30
closed_by: codex-0
commits:
- hash: 37ed50976
  summary: Erase compile-known partial applications
- hash: 3f90e7b5a
  summary: Pin curried DL7 prefix syntax
---

# Erase compile-known partial applications

## Description

Intern unsaturated compile-known calls, append later arguments, and erase the partial carrier into a direct first-order call before checked runtime Datalog. Model class: Direct high.

## Acceptance Criteria

- [x] Callable plus ordered bound arguments determines one partial identity.
- [x] Later application appends arguments in declared order.
- [x] Checked runtime rules contain only direct first-order relation calls.

## Tests Run

- [x] Curried generic focused test passes.
- [x] Complete V7 SWI and Tree-sitter gates pass.

## Implementation Notes

Plan milestone 9. Dynamic higher-order runtime dispatch remains outside this
slice.

## Resolution

### 2026-08-30T23:49:01Z · @codex-0

Immediate partial application carries callable identity and ordered bound arguments, then erases into one direct Pair call. Complete V7 SWI passed 23 of 23 and Tree-sitter passed 1 of 1.
