# Brief: extract rename, arc 2: the stops

Read `CLAUDE.md` and `AGENTS.md` in full first. Then
`plans/2026-08-27-extract-rename.PLAN.md` in full (contract :269-521, arcs
table :556, row 2 is yours), and PR #511 (`git show 7691acea4 --stat`) which
landed arc 1: `src/0_rename.rs`, `src/rename_cx.rs`, `src/lang/ts_rename.rs`,
`tests/4_rename_ts.rs`, `tests/fixtures/ts_rename/local/`.

## First action
```bash
git merge --ff-only 7691acea4b8caaf5a1b03d0d44974ac917dc1dfd   # STOP AND REPORT on failure
```

## Files you own
- `v6/sprefa-extract/src/lang/ts_rename.rs`, `src/0_rename.rs` (the `--at <byte>` flag and the stop-to-exit mapping only), `src/bin/extract.rs` (the `--at` flag on the rename verb only)
- `tests/4_rename_ts.rs` (append tests), new fixtures `tests/fixtures/ts_rename/stops/{ambiguous,not_found,inexact,dynamic}/src/app.ts`
- `tests/fixtures/kind_vocab/wire_golden.jsonl`: regenerate ONLY if the kind_vocab test tells you to, and say so in the commit body with the insertion/deletion counts (expect insertions only).
- new issue: `issuectl new -t feature --slug extract-rename-arc2-stops --title "extract rename: arc 2, the named stops" -a chris -p normal -l extract -l rename --description "plans/2026-08-27-extract-rename.PLAN.md arc 2"`; tick it as its own commit AFTER the code commit.
FORBIDDEN: `src/types.rs` (the `RenameStop` enum already has every variant; if one is missing, STOP and hail), `src/rename_cx.rs`, `src/lang/mod.rs`, `src/0_move.rs`, `src/move_*.rs`, `src/lang/*_rehome.rs`, `src/lang/ts.rs`, `src/scip*.rs`, everything under `v6/sprefa-engine-rs` and `v6/sprefa-store`. Never commit `Cargo.lock`.

## The four stops (plan row 2)
| stop | fixture `src/app.ts` | command | expected |
|---|---|---|---|
| `Ambiguous` | two declarations named `Foo` in one file (a `const Foo` inside one function body, and a top-level `class Foo`) | `extract rename src/app.ts#Foo Bar --commit` | exit non-zero, message names BOTH byte offsets; then `--at <offset of the class>` succeeds and renames the class and its uses only |
| `NotFound` | no `Foo` declared | same | exit non-zero, message `not found: Foo in src/app.ts` |
| `Inexact` | a use whose exact letters cannot be pinned (the plan names the case; if oxc_semantic pins every seat, prove it with a test that asserts the stop is unreachable from the TS arm and say so) | same | exit non-zero, offset in the message |
| `Dynamic` | `obj["Foo"]` and `import("./m").then(m => m.Foo)` beside a real `Foo` | same | exit non-zero, message lists each dynamic seat with file and offset |
Every stop: the tree after the run is byte-identical to the untouched copy (`diff -rq`, zero entries). No stop panics: `grep -n 'panic!\|unwrap()' src/lang/ts_rename.rs src/0_rename.rs` = 0 lines. Exit codes: one code per stop, documented in `0_rename.rs` next to the mapping.

## Fail-first
Write the four fixtures and tests first, run, paste each failing line in the commit body, then implement.

## Receipts (PR body)
- `cargo test -p sprefa-extract --features cli` in the FOREGROUND (never background): 0 failures, count; `4_rename_ts` count (arc 1 had 3).
- `git diff 7691acea4b8caaf5a1b03d0d44974ac917dc1dfd --stat`: only owned files.
- The `grep` for panic/unwrap above, verbatim.
- `cargo fmt`; no `eprintln!` in `src/**` beyond the 4 `@eprintln-ok` lines in `bin/extract.rs`; each test under 10 s.

## Style
Comment budget: constraints only. Banned words: provenance, substrate, load-bearing, regime, refusal, ground truth, "support". Descriptive identifiers. Async banned. Issue tick as its own commit AFTER the code commit.

## Delivery
One PR against `origin/main`, title `extract rename: arc 2, the named stops`. Do not merge. Hail on post and on block:
`boop beep --no-wait --as <your-lane-name> sprefa-coordinator "<PR#, test counts, panic/unwrap grep>"`.
