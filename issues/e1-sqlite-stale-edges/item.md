---
created: 2026-10-01
updated: 2026-10-01
type: bug
status: open
priority: normal
epic: emitter-v1-burndown
labels:
- size:med
---

# E1: sqlite engine keeps stale : rows after a later round removes support

## Description

src/_6_eval/_7_sqlite_eval.rs:1356-1407. Partial fix on fix/engine-e1-e2 f5a8e1391; it needs a recompute guard and a test file name other than _9g (taken by _9g_call_hand).
