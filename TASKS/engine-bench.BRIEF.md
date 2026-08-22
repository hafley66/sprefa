# Brief: the Rust door gets its own perf battery

Base sha: the spawner prints it. FIRST ACTIONS: `git merge --ff-only <sha>`, then
`bash v6/tools/doctor-deps.sh` (DEPS OK). Never spawn subagents. Commit every green step.
PR against `main`. `export CARGO_BUILD_JOBS=3 RUST_TEST_THREADS=4`; `timeout` on every
command; no single operation over 10s except cargo build and the 1M-row leg, which runs
in the background with its own cap.

## The gap, measured
`v6/justfile` has nine perf recipes and none touches `v6/sprefa-engine-rs`: `shootout` runs
`labs/exec_shootout/{interp,rxgraph,mono}` (toy engines), `dl6-bench*` the emitted PROLOG
runtime over `labs/exec_shootout/dl6/reachability.dl6`, `bench` and `perf-report` the TS-era
`sprefa-store`, `scale-floor`/`memory-soak`/`watch-scale` the paused `tsv2`. `grep
sprefa-engine-rs v6/labs/BENCHMARKS.md` is empty. The Rust door's only perf receipts are
7 statement-COUNT tests in `sprefa-engine-rs/tests/n_plus_one.rs` and today's ad hoc
tracing numbers (sf_join 54k rows 0.85s, `DL_TRACE_SUMMARY=1`).

## Build: `just engine-bench` and `just engine-bench-full`
Mirror `labs/exec_shootout/dl6/bench.sh` + `budget.json` + `budget-check.sh` exactly in
shape, for the Rust door, under `v6/sprefa-engine-rs/bench/`:
1. Programs (dl6, compiled through `v6/prolog/emit_rust.pl` as `v6/dl/deadcode/dead-module-rail.sh`
   does; reuse `reachability.dl6` byte-identical as the first program so the two doors are
   comparable): `reachability` (chain, grid, layered), `retraction` (same grid, then a
   schedule that retracts 1% of edges per tick for 20 ticks, the IVM workload), `join_heavy`
   (the `sf_join` shape from today's tracing, 54k rows, find it by `grep -rn sf_join
   v6/dl v6/sprefa-engine-rs/tests`), `log_keep` (a `log keep(all)` rel fed 10k rows/tick for
   50 ticks).
2. Scales 10k / 100k by default, 1M under `ENGINE_BENCH_FULL=1`.
3. Per leg, written as one JSON row to `bench/FACTS.json` and a table in `bench/FACTS.md`:
   compile_ms, load_ms, fixpoint_ms, per-tick wall p50/p95 over the schedule, statements per
   tick (from `DL_TRACE_SUMMARY=1`, parse the summary table; `src/trace.rs`), sqlite_ms share,
   peak RSS (`/usr/bin/time -l`, `maximum resident set size`), derived row count and a
   checksum of the final state (so the prolog door's `grid_10000 derived=1069200
   checksum=9d7239568960d6a8` can be compared on the shared program).
4. `bench/budget.json` ceilings per leg (fixpoint_ms, p95_tick_ms, peak_rss_mb,
   statements_per_tick), seeded from the first measured run times 1.25, rounded; `budget-check.sh`
   exits 2 on breach; ceilings ratchet DOWN only (a script refuses a raise).
5. Three runs per leg; FACTS carries min and the spread. Any leg over 10s at 100k is a
   finding, not a budget: name it in the PR with the trace table.
6. `v6/labs/BENCHMARKS.md`: one row per leg, same columns as the other engines.
7. `just engine-bench` wired into `perf-all` after `dl6-dred-bench`.

## Receipts in the PR
FACTS.md pasted; the prolog-vs-rust row for `grid_10000` side by side; `cargo test -q`
green (175/0 on main plus yours); `budget-check.sh` exit 0 on main; a deliberate ceiling
lowered to 1 shows exit 2 (paste), then restored.

## Ownership
Yours: `v6/sprefa-engine-rs/bench/**`, `v6/justfile` (append recipes only), `v6/labs/BENCHMARKS.md`,
`v6/sprefa-engine-rs/src/trace.rs` ONLY if the summary needs a machine-readable line
(then `DL_TRACE_SUMMARY=json`). FORBIDDEN: `v6/prolog/**`, `src/hosts.rs`, `src/run.rs`,
`src/executors/**`, `v6/dl/ghcache/**`, `v6/dl/prwatch/**`, `v6/tsv2/**`, `labs/exec_shootout/**`
(read only).

## Style laws
No em dashes. Banned: provenance, substrate, load-bearing, regime, refusal, "ground truth"
(say oracle). tracing only, no eprintln. Comment budget: constraints only. Failure ledger
entry: "a door with no bench".
