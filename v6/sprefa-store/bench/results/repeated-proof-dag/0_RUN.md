# Repeated comparison evidence

Measured code: `63d8b0034`. Both workloads used the existing `bench/run.sh`,
with one retained but discarded warmup followed by five measured runs. Each
run starts fresh engine processes and rotates engine order by three positions.
The existing chart pipeline plots per-cell medians. Full ranges remain in CSV.

- [DAG report](REPORT.md), [chart](retract_ms.png), [median rows](results.csv), [ranges](repeat-ranges.csv).
- [Cyclic report](../repeated-proof-cyclic/REPORT.md), [chart](../repeated-proof-cyclic/retract_ms.png), [median rows](../repeated-proof-cyclic/results.csv), [ranges](../repeated-proof-cyclic/repeat-ranges.csv).
- [Exact-state validation and counterexamples](../sequence-proof-final/0_FINDINGS.md).

## Verification

| Workload | Measured numeric rows | Excluded warmup rows | Matching input hashes | Status rows | Median groups |
|---|---:|---:|---:|---:|---:|
| DAG | 165 | 33 | 198 | 234 | 33 |
| Cyclic, stride 7 | 150 | 30 | 180 | 234 | 30 |

Every numeric cell has an `ok` adapter status, successful process exit,
matching canonical edge hash, and exact before/after validation outside the
clocks. Independent receipt checks recomputed node/edge/survivor counts and
all median columns. Each median group has five observations. There were zero
benchmark error, timeout, OOM, or resource-blocked statuses.

The 468 pre-cell OS memory samples ranged from 58% to 61% available memory.
The stop threshold was below 15% on two checks five seconds apart. This is
pre-cell sampling, not continuous total-RSS enforcement. The twelve disposable
PostgreSQL cluster lifetimes total 378.408 seconds, including warmups and
untimed setup/validation. Every cluster log records shutdown. Benchmark work
was sequential, with 120-second case limits and below the 20-minute budget.

At 160,002 nodes, retraction plus materialization/count medians are:

| Engine | DAG ms | Cyclic ms |
|---|---:|---:|
| native Differential Dataflow | 25.825 | 29.306 |
| SQLite count | 51.399 | unsupported |
| SQLite signed-delta-v2, full recomputation | 179.158 | 189.453 |
| native PostgreSQL, full recomputation | 231.033 | 246.357 |
| SQLite DRed loop | 278.194 | 308.212 |
| SQLite count-SCC | 278.879 | 307.445 |
| SQLite DRed CTE | 404.716 | 432.373 |
| PGlite, full recomputation | 416.780 | 442.891 |
| actual emitted TSV2 runtime | 681.254 | 717.583 |
| actual emitted Rust runtime | 990.015 | 1056.267 |
| SWI incremental | 1105 | 1163 |

The signed-delta-v2 name is retained for continuity. Its implementation
recomputes from implicit zero-indegree roots excluding only the current seed.
Its single-deletion cells above pass, while generalized sequence checks expose
21 failures, including a two-node, zero-edge repeated-deletion counterexample.
No production fix was made. Both pg_ivm arms retain explicit unsupported
recursive-query statuses. Plain counting retains unsupported cyclic statuses.

## Measurement boundaries and remaining uncertainty

All numeric arms time deletion, maintenance or recomputation/materialization,
and count. Exact ordered validation, hashing, and full PostgreSQL result
transfer are excluded. Native PostgreSQL includes client round trips and a
durable deletion commit, with `fsync=on`, `synchronous_commit=on`, and
`full_page_writes=on`. Specialized RelStore files use the existing SQLite
`journal_mode=WAL`, `synchronous=NORMAL` configuration. The emitted SQLite
runtimes use in-memory stores. These durability settings are not equivalent.

RSS is a mixture of adapter-specific process peaks and samples, including
untimed validation. PostgreSQL sums client/server process RSS and may count
shared mappings more than once. Requested memory limits do not uniformly cap
SQLite C heaps, PostgreSQL, or total RSS. Raw reports specify each scope.

The five observations and deterministic rotation establish repeatability for
these recorded workloads and host conditions. They do not establish a general
ranking, a statistically estimated crossover, or production capacity. The
finite correctness tests do not constitute formal implementation verification.
Rust trace evidence confirms expansion/support recount execution, but does not
establish a causal performance effect from its unused emitted DRed field.

## Reproduction

Run from `v6/sprefa-store` using a new output destination:

```bash
LC_ALL=C LANG=C POSTGRES_SHOOTOUT=1 DD_SHOOTOUT=1 SQLITE_SHOOTOUT=1 \
BENCH_REQUIRE_INPUT_HASH=1 BENCH_BACK_STRIDE=0 BENCH_REPEATS=5 \
BENCH_MIN_FREE_PERCENT=15 \
BENCH_ENGINE_FILTER='swi-incr differential-dataflow sqlite-count sqlite-count-scc sqlite-dred-loop sqlite-dred-cte sqlite-signed-delta-v2 tsv2-runtime sprefa-engine-rs pglite-query native-postgres-query pglite-pg_ivm native-postgres-pg_ivm' \
SCALES='2x200 6x2000 8x20000' BENCH_CELL_BUDGET_S=120 PG_BENCH_BUDGET_S=120 CAP=4096 \
BENCH_OUT=bench/results/NEW-REPEATED-DAG-DESTINATION bench/run.sh
```

For cyclic cells use stride 7 and a different new destination. Run sequentially.
The report links each raw run, whose directory contains logs, input hashes,
status rows, engine order, and resource samples. Existing receipts are rejected.

The shared DL6 source was recompiled to both target artifacts after these runs;
both artifacts were byte-identical to the measured versions. Ordinary Node
regressions passed 14 tests with the opt-in external sequence suite skipped;
the full external suite's separate pass/fail/unsupported accounting is linked
above. A follow-up DD observer-frontier check passed all 39 fixtures and 1,129
states in `../sequence-proof-dd-frontiers/`. The exhaustive counting check passed
2,048 cases, 6,144 states, and 24,576 stored-weight checks. These add executable
regression coverage; CI workflow configuration is unchanged.
