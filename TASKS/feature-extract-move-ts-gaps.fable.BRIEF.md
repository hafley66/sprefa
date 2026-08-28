# Brief: extract move (TypeScript) uncovered reference classes

You are a Fable coordinator lane. You fan out to opus and sonnet lanes
(`boop beep lane create --preset <opus|sonnet preset from config.json>`),
grade every lane tree yourself (git log, git status, gate), and post PRs.
Read `CLAUDE.md` and `AGENTS.md` in full first; every law there binds you and
every lane you spawn. Language/type design stays with the user; this is
tooling work only.

## First action

```bash
git merge --ff-only origin/main   # base sha printed at spawn; STOP AND REPORT on failure
```

## Where things are

- Command: `extract move` in `v6/sprefa-extract/src/0_move.rs`, TS resolver
  `v6/sprefa-extract/src/lang/ts_resolve.rs` (oxc_resolver), walker
  `v6/sprefa-extract/src/lang/ts_walk.rs`. Tests `v6/sprefa-extract/tests/1_move.rs`.
- Plan: `plans/2026-08-25-extract-move-typescript.PLAN.md` (+ `.visual.human.unga.md`).
- Issue with the trial receipt: `issues/extract-move-typescript/item.md`,
  "Agent Runs" section dated 2026-08-26 (codex). Reopen it with `issuectl`;
  issue edits commit straight on main (AGENTS.md "Issues (issuectl)").
- Trial trees: `/private/tmp/grapht-move-expected.iUMxzq` (reviewed result,
  `packages/` present). `/private/tmp/grapht-move-full.tY3V3k` is EMPTY; the
  tool-applied tree is gone. Rebuild it: hafley-rxjs pre-refactor commit
  `f427e81`, reviewed layout `00005e2` on branch
  `feature/generic-graph-rxjs-renderers` in `~/projects/hafley-rxjs`. Use a
  fresh `git worktree add --detach` under
  `~/projects/hafley-rxjs/.boop-worktrees/` ONLY; never touch existing
  worktrees, never push to their branches. 66-row TSV old<TAB>new from
  `git log --diff-filter=R --name-status -M f427e81..00005e2`.

## Trial result (codex, 2026-08-26)

66 moves, static TS import and export-from specifiers rewritten byte-equal to
the reviewed tree. 18 entries differ:

| class | count | example |
|---|---|---|
| `new URL(..., import.meta.url)` and `resolve`/`dirname` relative constants in moved TS | 8 files | |
| package-gate / lane root constant pair | 1 pair | |
| `package.json` export targets | 1 | |
| compiled-output paths in bin scripts | 3 | |
| `justfile` paths | 1 | |
| documentation references | 2 | |
| empty old `tests/helpers` dir | 1 | |

## Deliverable

One arc per class, each a PR against sprefa `origin/main`, smallest first:

1. Empty-directory cleanup after move (rmdir parents left empty, inside the
   same soopy StageRequest or immediately after commit; receipt in the move
   output).
2. `import.meta.url` / `resolve(__dirname, ...)` / `dirname(fileURLToPath(...))`
   relative path string literals in MOVED files: classify with the oxc AST
   (string literal argument to `new URL`, `resolve`, `join`, `fileURLToPath`
   chains), re-aim relative to the new location. No regex over source.
3. `package.json` `exports`/`main`/`types`/`bin` targets: typed JSON edit
   (serde_json, sorted keys, see `wire.rs`/`0_query.rs` for the preserve_order
   note), rewrite when the target file moved.
4. Bin scripts + justfile + markdown: a SEPARATE opt-in pass
   `extract move --text-refs` that reports (default) or rewrites (flag) exact
   old-path substrings in non-TS files. Report mode ships first with a golden;
   rewrite mode only if the user's plan decision (`PLAN.md` "Implementation
   Notes" line about manifests and script strings) is cited.

Each PR: fail-first test in `tests/1_move.rs` reproducing the Grapht class on
a fixture under `v6/sprefa-extract/tests/fixtures/`, then the fix, then the
Grapht diff count in the PR body (18 -> N). `cargo test -p sprefa-extract`
green, `cargo fmt`, no `eprintln!` in `src/**`.

## Lane ownership

- Lane A (opus): arcs 1 and 2. Owns `0_move.rs`, `ts_walk.rs`, `tests/1_move.rs`.
- Lane B (sonnet): arcs 3 and 4 report mode. Owns a NEW
  `src/1_move_manifest.rs` and `src/2_move_text.rs`, plus its own test file
  `tests/2_move_refs.rs`. Forbidden: `0_move.rs`, `ts_walk.rs`, `ts_resolve.rs`.
  Wire-in edits to `bin/extract.rs` go in a 5-line region B owns; A does not
  touch `bin/extract.rs`.
- Sequence B's PR after A's merge; rebase B yourself.

## Reporting

Hail `sprefa-coordinator` on every PR post and on every block, one line:
PR number, test counts, Grapht diff count. Final: update the issue's
"Agent Runs" with the per-arc receipts and the residual diff list, commit on
main, push.
