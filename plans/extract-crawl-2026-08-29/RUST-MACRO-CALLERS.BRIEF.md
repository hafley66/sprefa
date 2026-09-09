# Lane `fix/extract-rust-macro-callers` (glm53): census class M1, macro-minted callers

First action: `git merge --ff-only 21ca6d1b80bd28f933e611a65f4912901685c701`. Failure = STOP, beep the coordinator.

## Goal, one sentence

Close census class M1 (1,928 rows, 14.01% of the pre-sysroot miss set:
CALLERS minted by macro_rules so our caller_name is empty or wrong while
CodeQL names the expanded fn): re-census on the post-sysroot binary first
(the sysroot fix may have shrunk M1), then fix the biggest surviving
mechanism, fail-first, census count must drop, contradicted must not rise.

## Method

1. Re-run `plans/extract-bench-2026-08-29/rust.call_census.py` on a fresh
   release build (features cli,rust-checker). Commit the new class table to
   rust.REPORT.md (next free section). M1 may have moved: fix the LARGEST
   surviving M-class, state which and why.
2. Read 3 cited examples at path:line. The call plane already has MBE
   expansion (rust.REPORT.md sec 14 per OPEN-PROBLEMS row 11) — find why the
   caller side does not use it where CodeQL does.
3. Fail-first fixture test `tests/95_rust_macro_callers.rs`, smallest fix,
   census before -> after in the commit message.
4. A mechanism needing new checker queries or a design call (span of an
   expanded def, cross-crate naming) = write the stop in the report, beep the
   coordinator, do NOT redesign the seam.

## Files you own

- `v6/sprefa-extract/src/lang/rust.rs`, `rust_scip_macros.rs`,
  `rust_checker_ra.rs` (small), `tests/95_rust_macro_callers.rs` + fixtures
- `plans/extract-crawl-2026-08-29/rust.REPORT.md` one section,
  `plans/extract-bench-2026-08-29/OPEN-PROBLEMS.md` row 11
- FORBIDDEN: ts/go/python files, tests/bench/**, RATCHET.tsv (report only;
  floors move in a coordinator-verified bump later).

## Receipts

Census table before -> after (counts sum to the miss set); fail-first red
output; suite cli,rust-checker,ts-checker rc=0 direct; diff-stat owned only.
Measure signature on every percent (COMMON.md).

## Laws

Banned words prose+identifiers: provenance, substrate, load-bearing, regime,
ground truth (say oracle), honest, signal. Commit per step; push; `gh pr
create`; beep sprefa-coordinator with PR number + the class table.
