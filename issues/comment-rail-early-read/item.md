---
created: 2026-08-15
updated: 2026-08-15
type: bug
reporter: fable
status: open
priority: normal
---

# comment-budget rail reads violation_run before the waiver retraction tick

## Description

## Comments

### 2026-08-15T18:30:43Z · @fable

Quiescence heuristic (no tick for COMMENT_RAIL_IDLE_MS, default 700ms) can fire between the violation-minting host round and the waiver-join retraction round. Same staged index graded rc=0 standalone, rc=2 under the pre-commit hook, 3x each, flagging a line carrying @comment-ok. COMMENT_RAIL_IDLE_MS=3000 on the identical index resolved it. Fix direction: a real quiescence signal from serve (all host rounds answered) instead of idle-time. Ledger: docs/failure-modes.md #47.
