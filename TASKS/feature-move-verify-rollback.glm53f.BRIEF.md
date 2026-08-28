# Brief: `extract move --verify '<cmd>'` (issue move-verify-rollback)

Read `CLAUDE.md` and `AGENTS.md` in full first. Then
`plans/2026-08-27-extract-move-parity-v1-v5.PLAN.md` (port rank 1) and
`issues/move-verify-rollback/item.md`.

## First action

```bash
git merge --ff-only afa481059   # STOP AND REPORT on failure
```

## Files you own

- `v6/sprefa-extract/src/0_move.rs` (CLI flag + orchestration)
- `v6/sprefa-extract/src/move_stage.rs` (commit path)
- `v6/sprefa-extract/tests/1_move.rs` (append tests only)
- `issues/move-verify-rollback/item.md` (tick Acceptance Criteria, add an Agent Runs note)

FORBIDDEN: `src/lang/**`, `src/move_cx.rs`, `src/2_move_text.rs`, `src/types.rs`,
`src/bin/extract.rs`. Another lane owns `src/lang/rust_rehome.rs` right now.
If you need a change there, STOP and hail.

## What to build

`extract move ... --commit --verify '<shell command>'`:

1. Stage and commit the move exactly as today (one soopy StageRequest).
2. Run `<shell command>` with `sh -c` in the move root, stdout/stderr
   inherited, a hard timeout of 300 s (`std::process::Command` + wait with
   timeout; `wait-timeout` crate is acceptable, name it in the PR body).
3. Exit 0 from the command: print `verify ok` and finish.
4. Non-zero or timeout: restore every touched path to its pre-run bytes and
   location, print `verify failed (rc=N): rolled back <count> files`, exit 3.
   Rollback = the inverse StageRequest through soopy (Move new->old,
   Replace with the pre-run bytes you captured before commit, Delete for
   created shims, and re-create any directory the #484 sweep removed).
   Never `git checkout`; the root may not be a git tree.
5. `--verify` without `--commit` is an error naming both flags.
6. Dry run never runs the command.

Precedent to read, not copy: v5 `src/lib.rs:444 run_verify`,
`src/engine/query.rs:147-172` (begin_verify / commit_writes / rollback_writes)
on the repo root, and `tests/it/verify_rollback.rs`.

## Fail-first tests (`tests/1_move.rs`, each fails before the code)

- `verify_true_keeps_the_committed_move`
- `verify_false_rolls_the_tree_back_byte_identical` (hash every file
  under the fixture root before and after; also the directory the sweep
  removed is back)
- `verify_without_commit_is_an_error`
- `dry_run_never_runs_verify` (command is `touch marker`; marker absent)

## Receipts (PR body)

- `cargo test -p sprefa-extract --features cli`: full battery 0 failures; `1_move` 16.
- `git diff afa481059 --stat` shows only the owned files.
- `cargo fmt`; no `eprintln!` in `src/**` (bin lines carry `@eprintln-ok`).
- 10-second law on every test.

## Style

Comment budget: constraints only. Banned words: provenance, substrate,
load-bearing, regime, refusal, ground truth. Descriptive identifiers.

## Delivery

One PR against `origin/main`, title `extract move: --verify runs a checker and rolls back (rank 1)`.
Hail on post and on block:
`boop beep hail sprefa-coordinator --from <your-lane-name> --body "<PR#, test counts>"`.
Do not merge.
