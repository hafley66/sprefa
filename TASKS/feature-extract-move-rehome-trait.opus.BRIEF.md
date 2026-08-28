# Brief: extract move, one `Rehome` impl per language (arc 1 + arc 3)

Read `CLAUDE.md` and `AGENTS.md` in full first; every law binds. Then read
the plan in the primary tree (not yet on origin/main; copy both files into
your worktree under `plans/` verbatim and commit them in your PR):

- /Users/chrishafley/projects/sprefa/plans/2026-08-26-extract-move-rehome-trait.PLAN.md
- /Users/chrishafley/projects/sprefa/plans/2026-08-26-extract-move-rehome-trait.PLAN.visual.human.unga.md

User decision (Chris, 2026-08-26, non-negotiable): every language is its own
impl. No `match` or `if` on `CorpusLang`/`ExtractLang`/extension anywhere
in the move core. The core asks the roster; each impl answers for itself.

## First action

```bash
git merge --ff-only 172e21dfb   # STOP AND REPORT on failure
```

## Scope: plan arcs 1 and 3

1. `Rehome` trait in `v6/sprefa-extract/src/types.rs` beside `Source`
   (:1938) and `Resolve` (:1663), exactly the signatures in the plan
   (`ImportRef`, `Respell`, `import_refs`, `respell`, `manifest_refs`,
   `shim`). Interface-bound, `I`-prefix law does not apply to Rust traits;
   keep the plan's names.
2. Roster `rehomes()` + `rehome_for(path)` in `src/lang/mod.rs`, same
   first-match order law as `sources()` (:66).
3. `impl Rehome for TsSource` in `src/lang/ts.rs` (or a `ts/` submodule),
   absorbing `ts_walk.rs`, `ts_resolve.rs`, `ts_paths.rs`, and
   `1_move_manifest.rs` (package.json targets become `manifest_refs`).
   `impl Rehome for PrologSource` in `src/lang/prolog/_0_source.rs`,
   absorbing `prolog_edits`, `prolog_files`, the resolve rule at
   `0_move.rs:785`, and the shim as `Rehome::shim`.
4. `0_move.rs` becomes language-free per the plan pseudo-code: open
   `ProjectCx` once (files, manifests, reader; soopy
   `_7_source_tree.rs:59 snapshot` for the walk), ask the roster, build
   Respells, assert `(file, span)` uniqueness naming both impls on clash,
   one soopy StageRequest, then the #484 rmdir sweep and #487 `--text-refs`.
   Delete `1_move_manifest.rs`. `2_move_text.rs` stays (text is not a language).
5. `--shim` routes through `Rehome::shim`; ts returns None with a named
   error `"<lang> has no shim form"`.

## Receipts (all in the PR body)

- `git grep -n 'CorpusLang::\|is_ts(\|ExtractLang::' v6/sprefa-extract/src/0_move.rs` prints nothing.
- `git grep -n 'read_dir' v6/sprefa-extract/src/0_move.rs v6/sprefa-extract/src/lang/ts_walk.rs` prints nothing (file may be gone).
- `grep -c 'std::fs' v6/sprefa-extract/src/0_move.rs` <= 2 (rmdir sweep).
- `cargo test -p sprefa-extract --features cli`: `1_move` 12, `2_move_refs` 4, full battery 0 failures. Byte-identical outputs on every existing move golden.
- Grapht oracle: fresh detached worktree of `~/projects/hafley-rxjs` at
  `f427e81` under `~/projects/hafley-rxjs/.boop-worktrees/` ONLY, 66-row
  TSV from `git log --diff-filter=R --name-status -M f427e81..00005e2`,
  `extract move --list --commit`, `diff -rq` vs a detached tree at
  `00005e2`: exactly 7 entries, the same 7 as PR #487's body. Remove both
  worktrees after. Never touch existing hafley-rxjs worktrees or branches.
- `cargo fmt`, no `eprintln!` in `src/**` (bin CLI lines carry `@eprintln-ok`).
- 10-second law: any single test over 10 s is a defect to name, never wait out.

## Style

Comment budget: constraints only, no narrative. Banned words: provenance,
substrate, load-bearing, regime, refusal, ground truth. Descriptive
identifiers. No new `match` on language anywhere.

## Delivery

One PR against `origin/main`, title `extract move: one Rehome impl per
language (arc 1+3)`. Hail on post and on block:
`boop beep hail sprefa-coordinator --from <your-lane> --body "<PR#, test counts, Grapht count>"`.
Do not merge. Rust impl (arc 2) is a later lane; do not start it.
