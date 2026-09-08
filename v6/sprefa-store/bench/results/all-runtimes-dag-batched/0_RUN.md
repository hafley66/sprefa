# Shared runtime comparison audit

The existing `bench/run.sh`, `chart.sh`, and `report.sh` produced these receipts.
Measurement code: `66b27d686`, with TSV2 fixture batching fixed in `0b73938c0`.
No production algorithm or compiler semantics were changed.

- [DAG report and complete rows](REPORT.md), [retraction chart](retract_ms.png), [CSV](results.csv).
- [Cyclic report and complete rows](../all-runtimes-cyclic/REPORT.md), [retraction chart](../all-runtimes-cyclic/retract_ms.png), [CSV](../all-runtimes-cyclic/results.csv).
- [Initial DAG receipts](../all-runtimes-dag/REPORT.md) retain the two TSV2 SQL-variable-limit failures. The adapter now loads initial edges in 1,000-row batches, then roots. Retraction remains one root-deletion tick.

## Executed implementations

| Arm | Existing implementation reached by the adapter |
|---|---|
| differential-dataflow | `examples/dd_reach.rs`: native differential-dataflow 0.25.1 / timely 0.31.0 iterative semijoin, distinct, and probe convergence |
| sqlite-count | `examples/perf_report.rs --shared`: `RelStore::retract` |
| sqlite-count-scc | same entrypoint: `RelStore::retract_scc` |
| sqlite-dred-loop | same entrypoint: `RelStore::retract_dred` |
| sqlite-dred-cte | same entrypoint: `RelStore::retract_dred_cte` |
| sqlite-signed-delta-v2 | same entrypoint: `cascade::retract_signed_delta_v2` |
| tsv2-runtime | `tsv2/scripts/scale-bench.ts reach`: compiler-emitted `program.tick` through `IncrementalRuntime` |
| sprefa-engine-rs | `sprefa-engine-rs/examples/0_reach_bench.rs`: compiler-emitted program through `drive_tick_transacted` and `GenProgram::run_tick` |
| swi-incr | `bench/swi_reach.pl`: SWI incremental tabling |
| native-postgres-query / pglite-query | `bench/engines/1_postgres_reach.mjs`: ordinary recursive SQL, full recomputation and materialization |

Both emitted programs come from `bench/0_root_reach.dl6`, with roots 0 and 1,
the same layered edges, and deletion of root 0. Cyclic cells add back edges
using the existing `gen_multi_cyclic` rule with stride 7. The older
`tsv2_retract.sh` hand-authored ScratchStore SQL benchmark is preserved and
is not used as an emitted-runtime measurement.

The emitted Rust IR contains `dred_sql`. Source inspection finds its Rust
type and field in `sprefa-engine-rs/src/types.rs:485` and `:568`, with no
runtime consumer. The actual Rust recount path executes `expand_sql` rounds
and support reconciliation in `src/incremental.rs:2934`. TSV2 reads
`statement.dred_sql` in `tsv2/runtime/1_incremental.ts:682` and calls
`maintain_head_in_place`. This implementation difference is retained.

## Measured retraction plus count

One observation per arm and scale in each current run. Values are milliseconds.
At 160,002 nodes, DAG has 306,667 edges and 140,002 survivors; cyclic has
326,667 edges and 141,906 survivors.

| Engine | DAG | Cyclic, stride 7 |
|---|---:|---:|
| differential-dataflow | 28.989 | 32.073 |
| sqlite-count | 57.595 | unsupported |
| sqlite-signed-delta-v2 | 214.489 | 198.564 |
| native-postgres-query | 254.203 | 268.973 |
| sqlite-count-scc | 338.740 | 330.542 |
| sqlite-dred-loop | 408.675 | 333.594 |
| sqlite-dred-cte | 566.390 | 461.825 |
| pglite-query | 682.782 | 490.918 |
| tsv2-runtime | 733.520 | 785.388 |
| swi-incr | 1395 | 1492 |
| sprefa-engine-rs | 1998.850 | 1104.066 |

The timed boundary is delete, maintain or recompute/materialize, then count.
Exact ordered validation and input hashing are outside it. Native PostgreSQL
includes client round trips and the durable deletion commit. The emitted
SQLite runtime arms use in-memory stores. RSS combines process peaks and
samples, including untimed validation; PostgreSQL sums its process tree and
may double-count shared mappings. Cap enforcement differs by arm and does
not enforce a common total-RSS ceiling. Per-arm scopes are in each report.
These observations do not establish a statistically estimated crossover.

## Validation and retained unsupported cells

- DAG: 33 numeric rows, 33 canonical input hashes, 39 status rows.
- Cyclic: 30 numeric rows, 30 canonical input hashes, 39 status rows.
- Every numeric row has an `ok` status, successful process exit, exact shared
  input hash, and matching node, edge, and survivor counts. Adapters compare
  full before/after sets outside the timers. An independent regression checks
  SWI's actual edge and survivor sets against the shared BFS on four DAG/cycle
  fixtures; large SWI cells additionally compare incremental and cold results.
- No error, timeout, or OOM rows in these two current runs.
- Both pg_ivm arms explicitly retain unsupported recursive-query statuses at
  all three scales. Plain `sqlite-count` also retains unsupported cycle statuses.
- Regression execution passed: 12 Node tests including harness fault cases
  and SWI/BFS equivalence, the pg_ivm unsupported-status shell test, and the DD
  oracle test. Native DD/store and emitted Rust release builds passed; both
  target programs compiled from the same DL6 source. Regression coverage was
  added; no CI workflow changed.
- Historical receipts were preserved. Disposable PostgreSQL logs record
  shutdown. The four full-run cluster lifetimes sum to 115.324 seconds;
  small diagnostic cells and tests were additional. Each case was capped at
  120 seconds, with sequential execution below the 15-minute benchmark budget.

## Reproduction

Compile the shared source from the worktree root:

```bash
LC_ALL=C LANG=C bash v6/tools/run-capped.sh 120 swipl -q \
  -l v6/prolog/compile.pl -l v6/prolog/emit_rust.pl \
  -g "compile_dl6('v6/sprefa-store/bench/0_root_reach.dl6','v6/tsv2/gen/bench_root_reach.ts'),compile_dl6('v6/sprefa-store/bench/0_root_reach.dl6','v6/sprefa-store/bench/1_root_reach.program.rs',[emitter(emit_rust:emit_program)]),halt"
```

Build `dd_reach` and `perf_report` release examples in `v6/sprefa-store`, and
`0_reach_bench` in `v6/sprefa-engine-rs` using `CARGO_TARGET_DIR=target` and
`cargo build --offline --release --example ...`.

Run from `v6/sprefa-store`, selecting a **new** result destination:

```bash
LC_ALL=C LANG=C POSTGRES_SHOOTOUT=1 DD_SHOOTOUT=1 SQLITE_SHOOTOUT=1 \
BENCH_REQUIRE_INPUT_HASH=1 BENCH_BACK_STRIDE=0 \
BENCH_ENGINE_FILTER='swi-incr differential-dataflow sqlite-count sqlite-count-scc sqlite-dred-loop sqlite-dred-cte sqlite-signed-delta-v2 tsv2-runtime sprefa-engine-rs pglite-query native-postgres-query pglite-pg_ivm native-postgres-pg_ivm' \
SCALES='2x200 6x2000 8x20000' BENCH_CELL_BUDGET_S=120 PG_BENCH_BUDGET_S=120 CAP=4096 \
BENCH_OUT=bench/results/NEW-DAG-DESTINATION bench/run.sh
```

For cyclic cells, use `BENCH_BACK_STRIDE=7` and a second new destination.
Run sequentially. Existing receipt destinations are rejected.
