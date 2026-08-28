# Brief: extract rename, arc 1: TypeScript, anchor file only

Read `CLAUDE.md` and `AGENTS.md` in full first. Then ALL of
`plans/2026-08-27-extract-rename.PLAN.md` (the contract is at "## Contract",
signatures at ":274-445", arcs at ":556", receipts at ":577", scope fence at
":522"). User decisions (Chris, 2026-08-27): sibling `Rename` trait beside
`Rehome`; exact identifier spans from `oxc_semantic` for TS and `syn` for
Rust (arc 5, NOT yours); SCIP verify-only (arc 6, NOT yours). You implement
arc 1 exactly as the plan's arcs table row 1 states. Nothing from arcs 2-6.

## First action
```bash
git merge --ff-only 7be76330e60a3281001153474e58edf9472d7ee3   # STOP AND REPORT on failure
```

## Files you own
- new: `v6/sprefa-extract/src/rename_cx.rs`, `src/0_rename.rs`, `src/lang/ts_rename.rs`, `tests/4_rename_ts.rs`, `tests/fixtures/ts_rename/local/{before,after}/`
- edited: `src/types.rs` (APPEND ONLY: `Rename`, `RenameStop`, `SymbolRef`, `RefRole`, `RenameCx` re-export if the plan puts it there), `src/lang/mod.rs` (`renames()` + `rename_for` beside `rehomes()` :91), `src/lib.rs` (mod + re-export lines), `Cargo.toml` (`oxc_semantic = "0.135"`, one line), `src/bin/extract.rs` (the `rename` verb arm only)
- new issue: `issuectl new -t feature --slug extract-rename-arc1 --title "extract rename: arc 1, TS anchor-file rename over oxc_semantic" -a chris -p normal -l extract -l rename --description "plans/2026-08-27-extract-rename.PLAN.md arc 1"`; tick it as its own commit AFTER the code commit.
FORBIDDEN: `src/0_move.rs`, `src/move_*.rs`, `src/2_move_text.rs`, `src/lang/*_rehome.rs`, `src/lang/prolog/**`, `src/lang/rust.rs`, `src/lang/ts.rs`, `src/scip*.rs`, every existing test file, everything under `v6/sprefa-engine-rs` and `v6/sprefa-store`. Never commit `Cargo.lock` changes outside the `oxc_semantic` addition.

## Shape
The signatures in `PLAN.md:274-445` are the contract; copy them, do not redesign. If a signature cannot be implemented as written, STOP and hail with the line and the reason; do not improvise a different shape. Arc 1 scope: a symbol declared in the anchor file, every reference inside that file, no importers. An exported symbol in arc 1 still renames only the anchor file and prints one line `public: <name> is exported; importers are arc 3`. The stops (`RenameStop`) exist as the enum; arc 1 raises only `NotFound` and `Ambiguous` (two declarations, no `--at` yet: stop with both offsets).

## Fail-first receipt (from the arcs table)
`tests/fixtures/ts_rename/local/before/src/app.ts` declares `oldName` (a function), uses it 3 times in the same file (a call, a reference passed as a value, a shadowed inner `oldName` in a nested block that must NOT change), and a string `"oldName"` that must NOT change. `after/` is written BY HAND before any implementation. Test: copy `before/` to a temp dir, run `extract rename src/app.ts#oldName newName --commit`, `diff -rq` against `after/` = zero entries. Also: without `--commit`, the temp tree is byte-identical to `before/` and stdout carries the plan lines shown in `PLAN.visual.human.unga.md` "What you type". Fail-first: run the test with `TsSource` absent from `renames()`, paste the failing line (`no rename arm for src/app.ts`) in the commit body, then wire it.

## Receipts (PR body)
- `cargo test -p sprefa-extract --features cli` in the FOREGROUND (never background): 0 failures, count pasted; `4_rename_ts` count.
- `git diff 7be76330e60a3281001153474e58edf9472d7ee3 --stat`: only owned files.
- `git grep -n 'match .*ExtractLang\|SupportLang::' v6/sprefa-extract/src/0_rename.rs v6/sprefa-extract/src/rename_cx.rs` = 0 lines (no language switch in the core; the roster routes).
- `cargo tree -p sprefa-extract -i oxc_semantic --depth 1 | head -3` and the `Cargo.lock` diff line count (expect one new package block only).
- `cargo fmt`; no `eprintln!` in `src/**` beyond the 4 `@eprintln-ok` lines in `bin/extract.rs`; 10-second law: the test runs under 10 s, time pasted.

## Style
Comment budget: constraints only. Banned words: provenance, substrate, load-bearing, regime, refusal, ground truth, "support" (say refCount). Descriptive identifiers, never single letters. No `unwrap()` in non-test code; a stop is a `RenameStop`, never a panic. Async banned. Issue tick as its own commit AFTER the code commit.

## Delivery
One PR against `origin/main`, title `extract rename: arc 1, TS anchor-file rename over oxc_semantic`. Do not merge. Hail on post and on block:
`boop beep --no-wait --as <your-lane-name> sprefa-coordinator "<PR#, test count, the diff -rq receipt, oxc_semantic lock delta>"`.
