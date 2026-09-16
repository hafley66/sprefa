# Brief: extract rename, arc 4: --text-refs report

Read `CLAUDE.md` and `AGENTS.md` in full first. Then
`plans/2026-08-27-extract-rename.PLAN.md` (contract :269-521, arcs table :556 row 4 is yours), `src/2_move_text.rs` in full (the move verb's report-only scan you extend), `src/0_rename.rs` and `src/lang/ts_rename.rs` as landed by #511/#514/#516, and `types.rs:2204` (`Rename::text_spellings`, a default you override).

## First action
```bash
git merge --ff-only c35ae28a7bff21820e4e0afbe930fd1b096f762b   # STOP AND REPORT on failure
```

## Files you own
- `v6/sprefa-extract/src/2_move_text.rs` (add `report_rename(cx: &RenameCx, request: &RenameRequest)` beside `report`; the `scan` fn becomes shared, never duplicated)
- `src/0_rename.rs` (the `--text-refs` flag wiring only), `src/bin/extract.rs` (the flag on the rename verb only)
- `src/lang/ts_rename.rs` (`text_spellings` override only)
- `tests/4_rename_ts.rs` (append), `tests/fixtures/ts_rename/exports/{before,after}/README.md` (new, identical in both: it mentions `Foo` once in prose)
- new issue: `issuectl new -t feature --slug extract-rename-arc4-text-refs --title "extract rename: arc 4, --text-refs report" -a chris -p normal -l extract -l rename --description "plans/2026-08-27-extract-rename.PLAN.md arc 4"`; tick it as its own commit AFTER the code commit.
FORBIDDEN: `src/types.rs`, `src/rename_cx.rs`, `src/lang/mod.rs`, `src/lang/rust*.rs`, `src/lang/ts.rs`, `src/lang/*_rehome.rs`, `src/0_move.rs`, `src/move_*.rs`, `src/scip*.rs`, `tests/5_*`, `tests/6_kind_vocab.rs`, `tests/fixtures/kind_vocab/**`, `tests/fixtures/rust_rename/**`, everything under `v6/sprefa-engine-rs` and `v6/sprefa-store`. Never commit `Cargo.lock`. Another lane owns the Rust rename arm concurrently; do not touch its files.

## Shape
`extract rename src/lib.ts#Foo Baz [--commit] --text-refs` prints, after the plan (and after the staged write under `--commit`), one `text-ref <file>:<line> <matched> -> <proposed>` row per leftover spelling in a file the plan did not edit at that line, same format `2_move_text.rs:23` prints. Carriers excluded: every file+line the plan rewrote. Spellings: the bare name in `"Foo"`/`'Foo'`/`\`Foo\`` string literals and in `.md` prose; the TS arm's `text_spellings` returns `[(old, new)]` plus nothing else in this arc.

## Fail-first tests (append to `tests/4_rename_ts.rs`)
1. `text_refs_reports_the_string_and_the_readme`: on the arc-3 fixture with `--text-refs` (dry run), stdout has exactly two `text-ref` rows: `src/e.ts:1 "Foo" -> "Baz"` and `README.md:<line> Foo -> Baz`; no row for any file the plan edits.
2. `text_refs_never_writes`: with `--commit --text-refs`, the committed tree `diff -rq` against `after/` = zero entries (so `e.ts` and `README.md` are byte-identical to `before/`).
3. `without_the_flag_no_text_ref_rows`: same command minus the flag, zero `text-ref` rows.
Write first, paste failing lines in the commit body, then implement.

## Receipts (PR body)
- `cargo test -p sprefa-extract --features cli` in the FOREGROUND (never background): 0 failures, count; `4_rename_ts` count (arc 3 had 10).
- `git grep -n 'fn scan' v6/sprefa-extract/src` = exactly one line (`2_move_text.rs`).
- `grep -n 'panic!\|unwrap()' src/2_move_text.rs src/0_rename.rs src/lang/ts_rename.rs` = 0 lines.
- `git diff c35ae28a7bff21820e4e0afbe930fd1b096f762b --stat`: only owned files; `cargo fmt`; no new `eprintln!`; every test under 10 s.

## Style
Comment budget: constraints only. Banned words: provenance, substrate, load-bearing, regime, refusal, ground truth, "support". Descriptive identifiers. Async banned. Issue tick as its own commit AFTER the code commit.

## Delivery
One PR against `origin/main`, title `extract rename: arc 4, --text-refs report`. Do not merge. Hail on post and on block:
`boop beep --no-wait --as <your-lane-name> sprefa-coordinator "<PR#, test counts, fn scan grep>"`.
