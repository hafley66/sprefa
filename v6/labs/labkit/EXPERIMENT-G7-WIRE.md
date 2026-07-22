# G7 — wire ALL 11 engines into the hermetic 0_unified matrix

You own `v6/labkit/` ONLY. A parallel job owns `v6/sprefa-store/`; do not touch
it, and do not change how you CALL sprefa-store (its public API is fixed).

## State
`src/bin/0_unified.rs` is the hermetic per-process runner (one OS process per
cell, DL_MEMCAP_MB, staging dropped before tick, blake3 vs the workload oracle).
It currently wires only 4 of the 11 engines:
  wired:   ram-zset, sqlite-temporal (live)   ·   ram-reach, sqlite-reach (reach)
  MISSING: CascadeZset, Reconciler, SqlReconciler, DdReach, DdBfs,
           SalsaReconciler (feature with-salsa), SalsaRows (feature with-salsa)

## Task
Wire every remaining engine into `0_unified`'s child dispatch and the matrix, so
the hermetic report covers all 11. For each engine:
- map it to the workload(s) it legitimately runs (a live-set engine to `live`, a
  reachability engine to `reach`) — an engine belongs in a workload only if its
  digest is meant to match that workload's oracle. Read each `impl Experiment`
  in src/ to see which it models; do not force an engine into a workload it does
  not implement.
- dd (DdReach, DdBfs) is `#[cfg(feature = "with-dd")]`; salsa (SalsaReconciler,
  SalsaRows) is `#[cfg(feature = "with-salsa")]`. Feature-gate their match arms
  and their inclusion in the engine lists with the SAME cfgs so a default build
  still compiles and runs. The report must state which rows require which feature
  build (run `0_unified` under `--features with-dd,with-salsa` to fill them).

## Discipline (the user is emphatic)
- Interpret results firsthand. Identical-across-scale numbers are a red flag to
  disprove, not a flex — a flat column must be explained or it is a bug.
- Every newly-wired engine's digest MUST match the workload oracle
  (correct=true). An engine that comes up `false` on a workload it should model
  is a wiring bug to fix, not a row to ship red. If an engine legitimately cannot
  match (e.g. it models a different answer), leave it OUT of that workload and say
  so in the result — do not fake agreement.

## Validate
- `cargo build` and `cargo build --features with-dd,with-salsa` both green.
- `cargo run --bin 0_unified` (default) and once more with
  `--features with-dd,with-salsa` regenerate UNIFIED-REPORT.md with all engines;
  every reported cell correct=true (or documented-and-excluded per above).
- Do NOT weaken any existing engine or the oracle check.

## Commit
Leave changes STAGED (`git add -A v6/labkit`). Do NOT commit (pre-commit hook
execs `dl`, not on PATH; do not bypass). Coordinator commits.

## Result file
`v6/labkit/EXPERIMENT-G7-RESULT.md`: the final engine×workload matrix, which
engines you added to which workload and why, any engine deliberately excluded
from a workload (with reason), and the raw validation output. Terse.
