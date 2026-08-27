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

- [x] `extract move ... --commit --verify '<cmd>'` stages and commits the move exactly as today, then runs `<cmd>` under `sh -c` in the move root with stdout and stderr inherited.
- [x] A 300 s hard cap kills a hung checker (`wait-timeout` 0.2, bin-only).
- [x] Exit 0 prints `verify ok` and finishes.
- [x] Non-zero or timeout restores every touched path to its pre-run bytes and location, prints `verify failed (rc=N): rolled back <count> files`, and exits 3.
- [x] Rollback runs through soopy: Delete for the shim, Move new -> old, Replace with the captured pre-run bytes, plus re-creating the directory the #484 sweep removed. Never `git checkout`.
- [x] `--verify` without `--commit` is an error naming both flags.
- [x] A dry run never spawns the command.

## Tests Run

`cargo test -p sprefa-extract --features cli`, `tests/1_move.rs` 17 passed / 0 failed in 1.53 s. Five of those are new and all five FAILED against the pre-change tree.

## Agent Runs

### 2026-08-27 · move-verify-rollback-opus

`--verify` landed on the positional and the `--list` door. The rollback is captured pre-commit (`Rollback::capture`, `src/0_move.rs`): pre-run bytes keyed by each touched file's pre-run path, the Create paths, the directories the sweep removes, and the directories the destinations mint. Restore order is forced: recreate the swept directories, Delete the shim (it sits on a move's old path), Move new -> old, Replace the bytes, then remove any minted directory the rollback left empty. New tests: `verify_true_keeps_the_committed_move`, `verify_false_rolls_the_tree_back_byte_identical` (ts_move/paths: edits plus three moves plus one swept directory, whole-tree byte map compared before and after), `verify_false_deletes_the_shim_it_rolled_back`, `verify_without_commit_is_an_error`, `dry_run_never_runs_verify`.
