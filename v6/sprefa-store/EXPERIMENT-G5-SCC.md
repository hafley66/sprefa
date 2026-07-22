# G5 — productionize the counting engine (cycle-correct SCC nested fixpoint) + drive its cost down

Worktree owner: you own `v6/sprefa-store/` ONLY. A parallel job owns
`v6/labkit/`; do not touch it. Branch off v11.

## The settled design you are building toward (do NOT re-litigate)
`v6/DECISIONS.md` + `v6/plans/2026-07-19-v6-table-design.md:344-368` PIN it:
retraction = counting Z-set (weight = # supports), delete-at-zero, NO separate
verb. Recursion by weight arithmetic. **Cycles handled by SCC-scoped NESTED
FIXPOINT, NOT DRed.** DRed (`retract_dred`, `retract_dred_cte`) is LAB-COMPARISON
ONLY and stays. differential-dataflow / salsa NOT adopted (resident RAM = the v5
36GB nightmare). Do not reopen any of this.

## The gap you are closing
`cascade::retract` (plain counting) is FAST but WRONG on cycles: it leaves
phantom-cycle rows alive (a 2-node cycle keeps each other's weight at 1 forever
after the external support dies). PERF-REPORT.md flags this `NO` at every CYC
scale. The pinned fix is: decompose the affected support graph into SCCs; a plain
counting pass handles cross-SCC (DAG) edges; WITHIN each nontrivial SCC run a
nested fixpoint that detects "no external support reaches this SCC" and drops the
whole component. This makes counting cycle-correct WITHOUT DRed's two full passes.

## Deliverables
1. `cascade::retract_scc` (new fn, do NOT change the signature of existing
   `retract`/`retract_dred`/`retract_dred_cte`/`alive_keys`/`add_rows`/`add_deps`/
   `assert`/`conn`/`attach` on RelStore — the labkit job depends on those exact
   signatures). A new `scc` module in the store if you need Tarjan/nested-fixpoint
   helpers, kept on-disk / set-based in the cascade spirit (state on disk, not a
   resident Rust graph — that is the whole point).
2. Wire it as a NEW engine row (`sqlite-count-scc` or similar) in
   `examples/perf_report.rs`'s `Engine` set, so it appears in the hermetic matrix
   next to oracle/count/dred-loop/dred-cte/dd.
3. Regenerate `PERF-REPORT.md` (`cargo run --example perf_report`, hermetic,
   per-process, hash-verified). The new engine MUST show `yes` (correct) at every
   CYC scale where plain counting shows `NO`, byte-identical to the oracle.
4. THEN drive its Big-O down. Levers already profiled (measure each, keep only
   wins): replace `SELECT DISTINCT` temp-b-trees with `INSERT OR IGNORE` into a
   PK'd frontier table (`cx_next`); frontier ping-pong instead of re-scan; fuse
   per-round statements. Every change re-verified against the oracle hash.
5. Update `FINDINGS-AND-GAPS.md`: mark the SCC engine landed, record the measured
   cost vs plain counting and vs DRed, honestly.

## MEASUREMENT DISCIPLINE (the coordinator is emphatic about this)
- **Interpret your own results firsthand.** Do not dump a table and stop. State,
  in your result doc, what each column MEANS and whether the number is credible.
- **Distrust your first output.** Re-run. If a result looks good, assume it is
  wrong until a second independent check agrees.
- **Identical numbers across scales are a RED FLAG, not a flex.** If `rust_live`
  is 0.09 MB "flat" at 60k and 960k, or ms barely moves across a 16x scale jump,
  that is evidence the measurement is broken or the engine is not actually doing
  the work at scale — PROVE it is real (e.g. rust_live is flat because rows never
  enter Rust: verify by checking sqlite_hw and db MB DO scale) before reporting it
  as a property. A number that does not move when the input grows 16x must be
  explained or it is a bug.
- Correctness is blake3/hash vs the independent `benchgraph::oracle_survivors`.
  Same-process timing is not trusted; use the harness's per-process measurement.
- Keep the memcap gun pointed (`DL_MEMCAP_MB`). Report any abort honestly.

## Validate
- `cargo test -p sprefa-store` green (agreement.rs must still pass; add an SCC
  correctness case to it).
- `cargo run --example perf_report` regenerates PERF-REPORT.md; new SCC engine
  correct on cycles.
- `rg -n 'eprintln' src/` — no NEW eprintln (tracing only; the existing
  DL_CASCADE_TRACE line is grandfathered).

## Commit
Leave changes STAGED (`git add -A v6/sprefa-store`). Do NOT commit (pre-commit
hook execs `dl`, not on PATH, will fail; do not bypass). Coordinator commits.

## Result file
`v6/sprefa-store/EXPERIMENT-G5-RESULT.md`: what you built, the measured numbers
WITH your firsthand interpretation of each, what you re-ran to disprove your own
first result, and any number you could not fully explain. Terse but honest.
