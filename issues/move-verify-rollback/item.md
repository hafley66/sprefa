---
created: 2026-08-27
updated: 2026-08-27
type: feature
assignee: chris
status: done
priority: high
epic: extract-move-parity
labels: [extract, refactor, core]
closed: 2026-08-27
---

# extract move --verify: run a checker after commit, roll back on non-zero

## Description

v5 0294e9c2f run_verify (src/lib.rs:444). Core flag on 0_move.rs/move_stage.rs; soopy stage is the transaction. Plan: plans/2026-08-27-extract-move-parity-v1-v5.PLAN.md. Receipt: --verify false leaves the tree byte-identical; --verify true commits; test in tests/1_move.rs
