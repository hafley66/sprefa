---
created: 2026-08-27
updated: 2026-08-27
type: feature
assignee: chris
status: open
priority: high
epic: extract-move-parity
labels: [extract, refactor, core]
---

# extract move --verify: run a checker after commit, roll back on non-zero

## Description

v5 0294e9c2f run_verify (src/lib.rs:444). Core flag on 0_move.rs/move_stage.rs; soopy stage is the transaction. Plan: plans/2026-08-27-extract-move-parity-v1-v5.PLAN.md. Receipt: --verify false leaves the tree byte-identical; --verify true commits; test in tests/1_move.rs

## Acceptance Criteria

- [x] `extract move ... --commit --verify '<cmd>'` stages and commits the move exactly as today (one soopy StageRequest).
- [x] The checker runs with `sh -c` in the move root, stdout/stderr inherited, 300 s hard timeout.
- [x] Exit 0 prints `verify ok` and finishes.
- [x] Non-zero or timeout restores every touched path to its pre-run bytes and location, prints `verify failed (rc=N): rolled back <count> files`, exits 3.
- [x] Rollback is the inverse StageRequest through soopy: Move new->old, Replace with the pre-run bytes captured before commit, Delete for created shims, re-creates any directory the #484 sweep removed. No `git checkout`.
- [x] `--verify` without `--commit` is an error naming both flags.
- [x] Dry run never runs the command.

## Tests Run

- [x] `verify_true_keeps_the_committed_move`
- [x] `verify_false_rolls_the_tree_back_byte_identical`
- [x] `verify_without_commit_is_an_error`
- [x] `dry_run_never_runs_verify`
- [x] Full `cargo test -p sprefa-extract --features cli` battery 0 failures; `1_move` 16.

## Agent Runs

### 2026-08-27T04:40:39Z · @move-verify-rollback-flash4

Port rank 1 landed. `--verify` implemented on `0_move.rs`/`move_stage.rs` boundary (no `move_stage.rs` change needed): pre-run bytes captured from the edit stage before commit, checker runs via `sh -c` with a 300 s poll-based timeout, rollback is the inverse soopy StageRequest (Delete shim, Move new->old, Replace with pre-run bytes, recreate swept directories), exit 3 on failure. Timeout uses a `std` poll loop on `Child::try_wait` instead of the `wait-timeout` crate, so no new dependency. `cargo test -p sprefa-extract --features cli` 0 failures, `1_move` 16.
