# Lane `feat/extract-rust-checker-types` (opus): type_of answers through the ra seam

First action: `git merge --ff-only 21ca6d1b80bd28f933e611a65f4912901685c701`. Failure = STOP, beep the coordinator.

## Goal, one sentence

The rust checker answers TYPE references the way it answers calls: a second
answer kind (type_of / definition-of-type-reference) through the existing
`rust_checker_ra.rs` seam into the Type family, with `resolution_origin:
checker` and External suppression, measured against
`rust.oracle.type.typedecl.tsv` before any floor moves.

## Why

User want 2026-08-31: "can we get type data from compiler tier" — yes, wire
it. Current floor: rust.type.checker(sysroot) vs typedecl on
rust-analyzer(873f): recall 64.93% = 5,417/8,343... CAREFUL: recall
denominator is the ORACLE count (8,343 is ours; read RATCHET.tsv row —
64.93/98.26 measured at f58cc6666). OPEN-PROBLEMS row 11: 255 macro-minted
type dsts are exactly what ra sees post-expansion and syntax cannot; expect
those to be the first class the new answers close. The ts twin (#621) wired
both answer kinds day one; rust only has call answers.

## Design constraints

- Mirror the call path: plain-data answers Corpus(path, span) / External /
  absent; built in the same single workspace load (NO second load_workspace_at
  — wall is already 13.5 s, budget none for a second load).
- External suppresses the syntax type leg's name-match guess (copy the call
  suppression wiring in rust.rs / rust_type_edges.rs).
- New edges carry `resolution_origin: checker`. RATCHET.tsv untouched in this
  PR; measure and report, floors move in a follow-up bump after coordinator
  verification.

## Files you own

- `v6/sprefa-extract/src/lang/rust_checker_ra.rs`, `rust_checker.rs`,
  `rust_type_edges.rs`, `rust.rs` (suppression only), `src/project.rs` small
- `v6/sprefa-extract/tests/94_rust_checker_types.rs` (new, fail-first) +
  fixtures `tests/fixtures/rust_checker_types/`
- `plans/extract-crawl-2026-08-29/rust.REPORT.md` one new section (check head
  for next free number), `plans/extract-bench-2026-08-29/OPEN-PROBLEMS.md`
  rows 1/11 if numbers move
- FORBIDDEN: ts/go/python files, tests/bench/**, RATCHET.tsv.

## Receipts (measure signature on every percent)

1. Fail-first 94 red then green.
2. Suite `cargo test --features cli,rust-checker,ts-checker` rc=0 direct.
3. Measured table: rust.type syntax-vs-checker on rust-analyzer(873f), both
   as `recall x% = a/b oracle type-rows, precision y% = a/c emitted`, plus
   3-bucket, wall ms, rss MB. Release binary, one process per leg, nice -n 15,
   background, 15-min cap.
4. diff-stat = owned files only.

## Laws

Banned words prose+identifiers: provenance, substrate, load-bearing, regime,
ground truth (say oracle), honest, signal. Commit per step; push; `gh pr
create`; beep sprefa-coordinator with PR number + the table. NO ratchet run.
