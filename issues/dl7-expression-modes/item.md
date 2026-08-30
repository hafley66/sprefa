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
lane_seq: 7
collision: [v7-lowerer, v7-checker, v7-test]
blocked_by: ['@dl7-reverse-query-parity']
closed: 2026-08-30
closed_by: codex-0
commits:
- hash: 1641f78c3
  summary: Check DL7 expression projection modes
---

# Check expression modes and cardinality

## Description

Use relation return metadata and functional keys to admit determinate expression projection and diagnose ambiguous projection without restricting explicit relational calls. Model class: Direct high.

## Acceptance Criteria

- [x] Supplied expression inputs functionally determine the projected return.
- [x] Ambiguous expression projection produces one positioned diagnostic.
- [x] Explicit calls retain zero-or-many relational answers.

## Tests Run

- [x] Focused and complete V7 SWI tests pass.

## Implementation Notes

Plan milestone 8. Reuse checked relation key sets and authored-order mode
analysis rather than adding a second mode table.

## Resolution

### 2026-08-30T23:44:47Z · @codex-0

Return-derived key sets govern projected calls; full calls without return metadata retain two answers. Complete V7 SWI passed 22 of 22.
