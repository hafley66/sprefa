# Brief: `extract move --root` repeatable, one MoveCx per root (issue move-multi-root, rank 6)

Read `CLAUDE.md` and `AGENTS.md` in full first. Then
`plans/2026-08-27-extract-move-parity-v1-v5.PLAN.md` (rank 6),
`issues/move-multi-root/item.md`, and PR #497 (`gh pr view 497`) for the
current shape of `0_move.rs` / `move_stage.rs` (VerifyJournal).

User decision (Chris): every language is its own impl; no `match`/`if` on
language anywhere in the move core.

## First action
```bash
git merge --ff-only 2b89314ee   # STOP AND REPORT on failure
```

## Files you own
- `v6/sprefa-extract/src/0_move.rs`, `src/move_cx.rs`, `src/move_stage.rs`
- `tests/1_move.rs` (append), fixtures `tests/fixtures/multi_root/**`
- `issues/move-multi-root/item.md` (tick AC; commit that tick as its OWN commit, subject `issues: ...`)
FORBIDDEN: `src/lang/**`, `src/types.rs`, `src/move_scip.rs`, `tests/3_move_rust.rs`, `tests/4_move_kotlin.rs`, `tests/5_move_scip.rs`. Three lanes own `lang/rust_rehome.rs` right now.

## What to build
- `--root <dir>` repeatable. Today one root is derived from the first move (`plan_root`). With N roots: every `--list` row (or the single old/new pair) must fall under exactly one root; a row under none, or under two, is a named error before any stage runs.
- One `MoveCx` per root, one Plan per root, one soopy StageRequest per root, in root order. `--verify` runs ONCE after every root committed, from the first root's directory unless `--verify-cwd <dir>` says otherwise; failure rolls back every root, last root first.
- Dry-run output and receipts are prefixed `[root <dir>]` per line so a reader can tell roots apart.
- Zero roots given = today's behaviour, byte-identical output on every existing golden.

## Fail-first tests (`tests/1_move.rs`, fixture `multi_root/{alpha,beta}`, each a tiny prolog or ts corpus)
1. `two_roots_each_rewrite_their_own_importers`
2. `a_move_under_no_root_is_a_named_error_with_zero_edits`
3. `verify_failure_rolls_back_every_root` (hash both roots before/after)
4. `no_root_flag_is_byte_identical_to_before` (reuse an existing golden)

## Receipts (PR body)
- `cargo test -p sprefa-extract --features cli` in the FOREGROUND (never background; the harness ends your turn and boop scores the lane incomplete): full battery 0 failures; `1_move` 20.
- `git diff 2b89314ee --stat` shows only owned files.
- `git grep -n 'CorpusLang::\|ExtractLang::\|match .*lang' v6/sprefa-extract/src/0_move.rs v6/sprefa-extract/src/move_cx.rs v6/sprefa-extract/src/move_stage.rs` prints nothing.
- `cargo fmt`; no `eprintln!` in `src/**`; 10-second law on every test.

## Style
Comment budget: constraints only. Banned words: provenance, substrate, load-bearing, regime, refusal, ground truth. Descriptive identifiers.

## Delivery
One PR against `origin/main`, title `extract move: --root repeatable, one MoveCx per root (rank 6)`. Do not merge. Hail on post and on block:
`boop beep --no-wait --as <your-lane-name> sprefa-coordinator "<PR#, test counts>"`.
