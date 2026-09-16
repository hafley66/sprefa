# Brief: extract rename, arc 3: TypeScript across files

Read `CLAUDE.md` and `AGENTS.md` in full first. Then
`plans/2026-08-27-extract-rename.PLAN.md` in full (contract :269-521, arcs
table :556 row 3 is yours, receipts :577), and the landed arcs: #511 (arc 1,
`src/0_rename.rs`, `src/rename_cx.rs`, `src/lang/ts_rename.rs`,
`tests/4_rename_ts.rs`) and #514 (arc 2, the stops, `--at`, exit codes
3-6). Read `src/lang/ts_rehome.rs:191` (`import_refs`) and v1's BFS at
`~/projects/sprefa-archive-20260428/crates/watch/src/plan.rs:442`
(`rename_through_reexports`) before designing the importer walk.

## First action
```bash
git merge --ff-only ee64084f32b9908317e3d9bd2ab6ce151c769f74   # STOP AND REPORT on failure
```

## Files you own
- `v6/sprefa-extract/src/lang/ts_rename.rs`, `src/0_rename.rs`, `src/rename_cx.rs`
- `src/types.rs` APPEND/EXTEND ONLY inside the rename block (`Rename`, `RenameStop`, `SymbolRef`, `RefRole`): `RenameStop::Dynamic` may become `Dynamic(Vec<SymbolSeat>)` (arc 2 reported one seat per run because the variant holds one span; fix that here, and make arc 2's dynamic test assert BOTH seats)
- `tests/4_rename_ts.rs` (append), new fixture `tests/fixtures/ts_rename/exports/{before,after}/`
- new issue: `issuectl new -t feature --slug extract-rename-arc3-crossfile --title "extract rename: arc 3, TypeScript across files" -a chris -p normal -l extract -l rename --description "plans/2026-08-27-extract-rename.PLAN.md arc 3"`; tick it as its own commit AFTER the code commit.
FORBIDDEN: `src/lang/ts_rehome.rs` (read it, call it, never edit it), `src/lang/ts.rs`, `src/lang/mod.rs`, `src/0_move.rs`, `src/move_*.rs`, `src/2_move_text.rs`, `src/scip*.rs`, `tests/6_kind_vocab.rs`, `tests/fixtures/kind_vocab/**` (the golden is pinned by corpus.txt since #512; new fixtures never touch it), everything under `v6/sprefa-engine-rs` and `v6/sprefa-store`. Never commit `Cargo.lock`.

## Scope (plan row 3, verbatim intent)
Importers found through `TsRehome::import_refs`; per importer, its `ImportSpecifier` for `old` is located, its LOCAL binding re-walked through the same `oxc_semantic` scope walk arc 1 uses; re-export relays (`export { Foo } from`, `export * from`) enqueue their own importers (v1's BFS) with body usages added. An aliased import `{ Foo as Bar }` moves only the `Foo` seat; `Bar` and its uses stay. A file that only mentions `"Foo"` in a string is untouched (arc 4 reports it; you do nothing). The `public:` line from arc 1 goes away: an exported symbol now renames its importers, and the line is replaced by the plan's per-file counts (`PLAN.visual.human.unga.md` "What you type").

## Fixture (`tests/fixtures/ts_rename/exports/`)
`before/src/lib.ts` exports `Foo` (a class) with 2 body uses; five importers, exactly as the plan's row 3 lists: (1) `src/a.ts` bare `import { Foo }` + 2 uses, (2) `src/b.ts` `import { Foo as Bar }` + 3 uses of `Bar`, (3) `src/barrel.ts` `export { Foo } from "./lib"` and `src/c.ts` importing `Foo` from the barrel + 1 use, (4) `src/star.ts` `export * from "./lib"` and `src/d.ts` importing `Foo` from it + 1 use, (5) `src/e.ts` with only `const key = "Foo"`. `after/` written BY HAND before implementation. Also a `tsconfig.json` at the fixture root so `npx tsc --noEmit` runs.

## Fail-first tests (append to `tests/4_rename_ts.rs`)
1. `exported_symbol_renames_every_importer`: `extract rename src/lib.ts#Foo Baz --commit` over a copy of `before/` -> `diff -rq` against `after/` = zero entries.
2. `aliased_import_moves_only_the_imported_seat`: `src/b.ts` in the committed tree contains `import { Baz as Bar }` and exactly 3 `Bar` uses, zero `Baz` uses in its body.
3. `dry_run_prints_per_file_counts`: without `--commit`, stdout has one line per touched file with its use count (lib 3, a 3, b 1, barrel 1, c 2, star 0 or absent, d 2) and the tree is byte-identical to `before/`.
4. `dynamic_stop_lists_every_seat` (arc 2's test, widened): both `obj["Foo"]` and `m.Foo` offsets appear in the message.
5. `tsc_is_clean_on_the_committed_tree`: `npx tsc --noEmit -p <copy>` exits 0; `#[ignore]` with the measured time in the attribute if it exceeds 10 s, and run it once by hand for the PR body.
Run each first, paste the failing line in the commit body, then implement.

## Receipts (PR body)
- `cargo test -p sprefa-extract --features cli` in the FOREGROUND (never background): 0 failures, count; `4_rename_ts` count (arc 2 had 6).
- `diff -rq` zero entries, pasted; `npx tsc --noEmit` exit code and time.
- `grep -n 'panic!\|unwrap()' src/lang/ts_rename.rs src/0_rename.rs src/rename_cx.rs` = 0 lines.
- `git diff ee64084f32b9908317e3d9bd2ab6ce151c769f74 --stat`: only owned files; `cargo fmt`; no new `eprintln!`; each test under 10 s or `#[ignore]`d with its time.

## Style
Comment budget: constraints only. Banned words: provenance, substrate, load-bearing, regime, refusal, ground truth, "support". Descriptive identifiers. Async banned. A stop is a `RenameStop`, never a panic. Issue tick as its own commit AFTER the code commit.

## Delivery
One PR against `origin/main`, title `extract rename: arc 3, TypeScript across files`. Do not merge. Hail on post and on block:
`boop beep --no-wait --as <your-lane-name> sprefa-coordinator "<PR#, test counts, diff -rq and tsc receipts>"`.
