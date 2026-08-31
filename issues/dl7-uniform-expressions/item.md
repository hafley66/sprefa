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
lane_seq: 4
collision: [v7-lowerer, v7-checker, v7-test]
blocked_by: ['@dl7-nested-applications']
closed: 2026-08-30
closed_by: codex-0
commits:
- hash: 27b621fb3
  summary: Use one DL7 expression lowerer in rules
---

# Use one expression lowerer in rules

## Description

Apply the same nested-expression lowering in bind targets, rule heads, and rule body arguments while preserving safety and origins. Model class: Direct high.

## Acceptance Criteria

- [x] Bind targets, head arguments, and body arguments share one lowerer.
- [x] Generated goals are hoisted to the nearest enclosing rule body.
- [x] Head safety and aggregate placement checks remain exact.

## Tests Run

- [x] Focused and complete V7 SWI tests pass.

## Implementation Notes

Plan milestone 5. Inspect `count` head syntax before changing nested-form
dispatch.

## Resolution

### 2026-08-30T23:38:55Z · @codex-0

Bind, head, body, negative-goal, and count positions now share expression lowering. Complete V7 SWI passed 20 of 20 and Tree-sitter passed 1 of 1.
