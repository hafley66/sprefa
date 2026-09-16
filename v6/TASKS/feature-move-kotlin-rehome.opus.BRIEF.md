# Brief: `impl Rehome for KotlinSource` (issue move-kotlin-rehome, rank 4)

Read `CLAUDE.md` and `AGENTS.md` in full first, then `plans/2026-08-27-extract-move-parity-v1-v5.PLAN.md`, `plans/2026-08-26-extract-move-rehome-trait.PLAN.md`, and PR #489 (`gh pr view 489`). User decision (Chris): every language is its own impl; no `match`/`if` on language anywhere in the move core (`src/0_move.rs`, `src/move_cx.rs`, `src/move_stage.rs`). Those three files plus `src/types.rs` Rehome trait, `src/lang/rust_rehome.rs`, `tests/1_move.rs`, `tests/3_move_rust.rs` are FORBIDDEN: six shootout lanes own them right now. If you need a change there, STOP and hail with the exact line. Style: comment budget = constraints only; banned words provenance, substrate, load-bearing, regime, refusal, ground truth; descriptive identifiers; `cargo fmt`; no `eprintln!` in `src/**`; 10-second law on every test. Delivery: one PR against `origin/main`, do not merge, hail on post and on block: `boop beep --no-wait --as <your-lane-name> sprefa-coordinator "<PR#, test counts>"`.

## First action
```bash
git merge --ff-only afa481059   # STOP AND REPORT on failure
```

## Files you own
- new `v6/sprefa-extract/src/lang/kotlin_rehome.rs` (mirror `ts_rehome.rs` / `prolog/_1_rehome.rs` shape)
- `src/lang/kotlin.rs` visibility widenings only (`kt_walk_import_headers` at :629 is the import walk to reuse)
- `src/lang/mod.rs`: ONE roster line adding `&KotlinSource` to `rehomes()`; `lib.rs`/`mod.rs` wiring for the new file
- new `tests/4_move_kotlin.rs`, fixtures `tests/fixtures/kotlin_move/**`
- `issues/move-kotlin-rehome/item.md`: tick Acceptance Criteria, add an Agent Runs note

## What to build (v5 precedent: repo root `src/ktpath.rs:1-115`, `src/lib.rs:1409-1412,1542-1602`, cases `tests/it/move_refactor.rs:371,403,464`)
- import_refs: every explicit `import a.b.Decl` whose target file is in the batch, kind `"import"`; the moved file's own `package a.b` declaration span, kind `"package_decl"`.
- respell: source root = old path minus declared package dirs (a disagreement between layout and `package` is a named error, never a guess); new package = new path minus source root; rewrite importers' `a.b.Decl` to the new package; rewrite the moved file's `package` line.
- Wildcard imports (`import a.b.*`) and same-package bare uses: count them and put the counts in the dry-run output as `warn` lines; do not rewrite.
- manifests: none. shim: None with error `"kotlin has no shim form"`. text_spellings: empty.

## Fail-first tests (each fails before the impl; cite the failing assertion line in the PR body)
1. explicit importer respelled across packages
2. moved file's package line respelled
3. wildcard importer warned, untouched
4. layout/package disagreement is a named error with zero edits
5. dry run prints every respell and touches nothing

## Receipts (PR body)
- `cargo test -p sprefa-extract --features cli`: full battery 0 failures; `4_move_kotlin` 5.
- `git diff afa481059 --stat` shows only owned files.
- `git grep -n 'CorpusLang::\|ExtractLang::\|match .*lang' v6/sprefa-extract/src/0_move.rs v6/sprefa-extract/src/move_cx.rs v6/sprefa-extract/src/move_stage.rs` prints nothing.
PR title: `extract move: Rehome for Kotlin (rank 4)`.
