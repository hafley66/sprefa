# Shared PostgreSQL / Differential Dataflow / SWI audit

Measured source: `2bf2ff8b0`, on 2026-09-07. The shared harness produced
12 numeric rows and 6 explicit recursive `pg_ivm` unsupported statuses.
The disposable PostgreSQL cluster ran from 22:28:58 to 22:29:10 EDT and
its retained log records shutdown. A process check found no remaining
`sprefa-store-pg` PostgreSQL process.

Run from `v6/sprefa-store`, selecting a fresh output directory:

```bash
LC_ALL=C LANG=C \
POSTGRES_SHOOTOUT=1 DD_SHOOTOUT=1 \
BENCH_ENGINE_FILTER='swi-incr differential-dataflow pglite-query native-postgres-query pglite-pg_ivm native-postgres-pg_ivm' \
SCALES='2x200 6x2000 8x20000' \
BENCH_CELL_BUDGET_S=120 PG_BENCH_BUDGET_S=120 CAP=4096 \
BENCH_OUT=bench/results/<new-directory> bench/run.sh
```

The [generated report](REPORT.md), [numeric rows](results.csv),
[status receipts](adapter-status.tsv), and [retract chart](retract_ms.png)
come from the existing `run.sh`, `report.sh`, and `chart.sh` path.
Every cell retains stderr, stdout, and its exit code under `logs/`.
The historical receipt directories remain byte-identical to `2743b5f14`.

Timing audit:

- The SWI retract clock now resets after initial-set validation and immediately
  before deleting root 0. The prior source included initial validation in the
  elapsed retract interval. This run excludes it.
- PostgreSQL and PGlite time deletion, recursive SQL materialization into a
  temporary table, and counting. Ordered full-result transfer, checksums,
  exact validation, and snapshot disposal occur outside these clocks.
- Native Differential Dataflow 0.25.1 over timely 0.31.0 executes
  `iterate`, `semijoin`, `distinct`, consolidation, and a progress probe.
  Deletion supplies a negative root update. The timer ends after the probe
  reaches the new frontier and the output accumulator is counted.
- SWI incremental tabling materializes and counts `alive/1` before stopping
  each clock. Initial IDs are checked against the complete generated node
  range; incremental survivor IDs are checked against cold table recomputation.
- All three principal arms therefore include delete, maintenance or full
  recomputation, materialization, and count. SQL additionally includes client
  round trips and a durable root-delete commit. Runtime initialization is
  excluded for SQL and SWI; DD's setup includes constructing its dataflow.

Verification executed:

```bash
node --test bench/run.test.mjs bench/engines/1_postgres_reach.test.mjs
bash bench/engines/3_postgres_ivm_unsupported.test.sh
CARGO_TARGET_DIR=target cargo test --offline --example dd_reach
CARGO_TARGET_DIR=target cargo build --offline --release --example dd_reach
```

The Node command passed 9 tests, including the parent test containing five
harness scenarios. The unsupported-status script passed, the DD example
passed its oracle test, and its release build succeeded. New regression
coverage exercises failure after CSV, explicit adapter error after CSV,
timeout after CSV, absent results, and preservation on attempted overwrite.
No CI workflow was changed.

Additional read-only checks validated all 12 numeric rows against the generated
node/edge/killed counts, checked successful raw exit codes, and independently
recomputed the SQL checksums. Actual SWI before/after ID sets matched the
JavaScript BFS oracle for `1x1`, `1x4`, `2x5`, `3x4`, `3x7`, and `8x20`.
The measured DD and SQL arms validate their complete sets against BFS in each
cell. The retract chart was visually inspected.

Limits: one observation per cell, one layered-DAG workload, roots `{0,1}`,
one root deletion, and no repeated-run uncertainty estimates. SWI wall time
has millisecond resolution. The optional pure-Prolog reference is preserved
but omitted here because it records CPU time and lacks exact-set validation.
RSS mixes adapter-specific process peaks and samples after validation;
native PostgreSQL sums client and server-tree RSS and can double-count shared
mappings. DD caps live Rust allocations, PGlite caps Node old-space, and
neither PostgreSQL nor SWI enforces the requested total-memory budget.
The `ops` field is engine-specific: DD counts output differences, SQL reports
`N/A`, and SWI emits a zero placeholder. It is not a cross-engine work count.

Versions observed: SWI-Prolog 10.0.2 (arm64-darwin), Node v24.15.0,
rustc 1.100.0-nightly (17fd5b8a3 2026-08-28), native PostgreSQL 18.6,
and PGlite's embedded PostgreSQL 18.3.
