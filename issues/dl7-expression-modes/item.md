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
lane_seq: 7
collision: [v7-lowerer, v7-checker, v7-test]
blocked_by: ['@dl7-reverse-query-parity']
---

# Check expression modes and cardinality

## Description

Use relation return metadata and functional keys to admit determinate expression projection and diagnose ambiguous projection without restricting explicit relational calls. Model class: Direct high.

## Acceptance Criteria

- [ ] Supplied expression inputs functionally determine the projected return.
- [ ] Ambiguous expression projection produces one positioned diagnostic.
- [ ] Explicit calls retain zero-or-many relational answers.

## Tests Run

- [ ] Focused and complete V7 SWI tests pass.

## Implementation Notes

Plan milestone 8. Reuse checked relation key sets and authored-order mode
analysis rather than adding a second mode table.
