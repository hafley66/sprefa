---
created: 2026-08-30
updated: 2026-08-30
type: task
status: in-progress
priority: high
epic: dl7-expression-flow
labels: [compiler]
size: L
lane: dl7-expression-flow
lane_seq: 4
collision: [v7-lowerer, v7-checker, v7-test]
blocked_by: ['@dl7-nested-applications']
---

# Use one expression lowerer in rules

## Description

Apply the same nested-expression lowering in bind targets, rule heads, and rule body arguments while preserving safety and origins. Model class: Direct high.

## Acceptance Criteria

- [ ] Bind targets, head arguments, and body arguments share one lowerer.
- [ ] Generated goals are hoisted to the nearest enclosing rule body.
- [ ] Head safety and aggregate placement checks remain exact.

## Tests Run

- [ ] Focused and complete V7 SWI tests pass.

## Implementation Notes

Plan milestone 5. Inspect `count` head syntax before changing nested-form
dispatch.
