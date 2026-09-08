# PostgreSQL shared shootout integration

Run date: 2026-09-07. Branch: `feature/postgres-ivm-crossover`. Base:
`c27c0c007`.

## Integration contract

The primary entrypoint is `v6/sprefa-store/bench/run.sh`. PostgreSQL-family
adapters participate in its existing engine loop, `layers x width` graph,
root-0 retraction, CSV schema, chart generator, and generated report.

For every cell, an adapter must:

1. Generate the existing DAG exactly, including roots 0 and 1.
2. Materialize the reachable-node set from both roots during setup.
3. Delete root 0 and materialize the survivor set from root 1.
4. Compare both complete ordered sets with an independent BFS oracle.
5. Emit the existing 11-column CSV row only after exact agreement.

Ordinary PostgreSQL and PGlite use recursive SQL full recomputation. Setup and
retraction remain separate timing phases. `pg_ivm` recursive maintenance is an
explicit unsupported status because pg_ivm rejects recursive view definitions.

The existing default run remains unchanged. PostgreSQL-family arms are opt-in,
and opt-in receipts use a new destination so prior lab receipts remain
byte-identical. A disposable native cluster and private Unix socket are scoped
to one harness invocation. PGlite data directories are disposable and scoped
to individual cells.

## Pre-implementation inventory

The current engine array contains `swi-incr`, `swipl-pure`, `swi-sqlite`,
`swi-ts`, `swi-emit`, `tsv2-gen`, and `v1-gen`. It does not contain a Rust
Differential Dataflow executable. `report.sh` has stale prose naming `dd` and
`dbsp`; generated reporting must identify the engines actually present and
distinguish full-recomputation arms.

## Executed result

The existing shared pipeline ran five compatible preexisting root-reachability
arms together with ordinary native PostgreSQL and PGlite. It produced 20
numeric rows across the first three default ladder cells. The `swi-ts`
160,002-node cell reached its 120-second process-group timeout; its 402-node and
12,002-node cells completed. Both PostgreSQL arms completed all three cells and
matched the independent BFS oracle for every complete before and after set.

The main generated report is
[REPORT.md](../v6/sprefa-store/bench/results/postgres-shared/REPORT.md). Raw
numeric rows are
[results.csv](../v6/sprefa-store/bench/results/postgres-shared/results.csv),
and supported, unsupported, timeout, semantics, and memory-scope records are in
[adapter-status.tsv](../v6/sprefa-store/bench/results/postgres-shared/adapter-status.tsv).
The existing chart pipeline generated
[retract_ms.png](../v6/sprefa-store/bench/results/postgres-shared/retract_ms.png),
[setup_ms.png](../v6/sprefa-store/bench/results/postgres-shared/setup_ms.png),
[rss_mb.png](../v6/sprefa-store/bench/results/postgres-shared/rss_mb.png), and
[ops.png](../v6/sprefa-store/bench/results/postgres-shared/ops.png).

## Exact shared command

Executed from the repository root:

```bash
cd v6/sprefa-store
../tools/run-capped.sh 900 env \
  POSTGRES_SHOOTOUT=1 \
  BENCH_ENGINE_FILTER='swi-incr swipl-pure swi-sqlite swi-ts swi-emit pglite-query native-postgres-query pglite-pg_ivm native-postgres-pg_ivm' \
  SCALES='2x200 6x2000 8x20000' \
  BENCH_OUT='bench/results/postgres-shared' \
  BENCH_CELL_BUDGET_S=120 \
  PG_BENCH_BUDGET_S=120 \
  CAP=4096 \
  bench/run.sh
```

The outer process-group bound was 900 seconds. Every cell had a 120-second
process-group bound. The run completed in about two minutes. Benchmark
processes ran sequentially.

The opt-in flag appends the PostgreSQL-family adapters. `BENCH_ENGINE_FILTER`
selects the common root-reachability workload. `tsv2-gen` and `v1-gen` remain
in the default engine array, but their wrappers reinterpret `layers` as a
program shape and `width` as EDB row count. They were excluded from this
command because they do not execute the root-retraction graph contract.

## Shared measurements

Each value is one measured process. The existing harness does not perform
warmup repetitions. PostgreSQL process startup and PGlite initialization occur
before `setup_ms`.

| nodes | engine | setup ms | retract and recount ms | sampled RSS MB | status |
|---:|---|---:|---:|---:|---|
| 402 | `swi-incr` | 1 | 1 | 10.75 | numeric |
| 402 | `swipl-pure` | 0.732 | 0.086 | 9.2 | numeric |
| 402 | `swi-sqlite` | 12 | 0 | 9.69 | numeric |
| 402 | `swi-ts` | 9.699 | 9.528 | 79.8 | numeric |
| 402 | `swi-emit` | 4.935 | 2.421 | 128.7 | numeric |
| 402 | `pglite-query` | 13.469 | 2.723 | 1092.9 | numeric |
| 402 | `native-postgres-query` | 11.481 | 3.087 | 104.2 | numeric |
| 12,002 | `swi-incr` | 80 | 70 | 44.47 | numeric |
| 12,002 | `swipl-pure` | 28.652 | 7.077 | 17.2 | numeric |
| 12,002 | `swi-sqlite` | 100 | 12 | 13.19 | numeric |
| 12,002 | `swi-ts` | 2391.333 | 1975.606 | 135.1 | numeric |
| 12,002 | `swi-emit` | 33.388 | 14.362 | 132.2 | numeric |
| 12,002 | `pglite-query` | 86.681 | 42.767 | 1097.3 | numeric |
| 12,002 | `native-postgres-query` | 133.029 | 30.386 | 171.3 | numeric |
| 160,002 | `swi-incr` | 1260 | 1330 | 634.83 | numeric |
| 160,002 | `swipl-pure` | 440.621 | 138.331 | 130.6 | numeric |
| 160,002 | `swi-sqlite` | 1458 | 215 | 33.28 | numeric |
| 160,002 | `swi-ts` | - | - | - | timeout at 120 seconds |
| 160,002 | `swi-emit` | 423.009 | 224.399 | 211.0 | numeric |
| 160,002 | `pglite-query` | 1123.806 | 834.435 | 1129.4 | numeric |
| 160,002 | `native-postgres-query` | 1082.350 | 532.112 | 298.7 | numeric |

The ordinary SQL timing boundary is:

- `setup_ms`: schema reset, root and edge table creation, deterministic edge
  generation, edge index creation, the initial recursive full query,
  materialization, ordered client transfer, checksum, and exact-set validation.
- `retract_ms`: `BEGIN`, root-0 deletion, `COMMIT`, the recursive full query,
  materialization, ordered client transfer, checksum, and exact-set validation.
- Process startup, database open, version query, final disk-size query, and
  database close are outside those phase values.

The native and PGlite checksums were identical for every scale:

| scale | before SHA-256 | after SHA-256 |
|---|---|---|
| `2x200` | `ada7ea43e998e3ba1357b4270279f2976e2659c6e3dd768d0fbfc6a15cd11314` | `d290b0a93c2138767799bffe561de2bad1a45c0ffc8210a3be393f16eb7bc305` |
| `6x2000` | `5359ff08f251a47ab81d104beeff9633f95d50c30ea2ff1db9d40617983ca17b` | `e640e84f06cd3d340d290625432f23bf9cf4b9df50d6e4ae1ab7a73f216df46b` |
| `8x20000` | `d5b111c00b9de6c6acc308845af36202735166f81b10ef25547078a8e03fcd77` | `ba46515280fa0525997e4d27a81f4d2dfdb3f4445c1e54d0cc969e59cb068bf2` |

## Supported, unsupported, and memory scope

| arm or condition | status | detail |
|---|---|---|
| native PostgreSQL 18.6 ordinary query | supported and executed | three shared scales, exact before and after sets |
| PGlite PostgreSQL 18.3 ordinary query | supported and executed | three shared scales, exact before and after sets |
| native pg_ivm recursive reachability | unsupported | pg_ivm rejects recursive view definitions |
| PGlite pg_ivm recursive reachability | unsupported | pg_ivm rejects recursive view definitions |
| `swi-ts`, `8x20000` | timeout | killed with its process group after 120 seconds; no numeric row |
| OOM | none observed | no OOM receipt occurred |
| missing PostgreSQL-family dependency | none observed | task-local native and PGlite dependencies were present |

`CAP=4096` is a request carried through the existing harness. It is not a
total-memory hard cap for either PostgreSQL-family adapter. For PGlite it limits
Node old-space only; the reported RSS samples the Node process containing the
PostgreSQL WASM runtime. For native PostgreSQL it is unenforced; reported RSS is
the sampled sum of the Node client and disposable PostgreSQL process tree, and
shared mappings may be counted more than once. Host page cache and swap are not
attributed. The adapter status receipt records these scopes per numeric cell.

## V7 runtime shootout integration

`v7/labs/18_runtime_shootout/4_run.sh` now accepts the same
`POSTGRES_SHOOTOUT=1` opt-in. A bounded N=16 smoke executed its six existing
arms, native PostgreSQL, and PGlite over both chain and ring. The complete smoke
receipt is
[6_POSTGRES_SMOKE.jsonl](../v7/labs/18_runtime_shootout/6_POSTGRES_SMOKE.jsonl).

Exact command:

```bash
cd v7
../v6/tools/run-capped.sh 120 env POSTGRES_SHOOTOUT=1 \
  ./labs/18_runtime_shootout/4_run.sh smoke 16
```

| arm | chain closure ms, 120 pairs | ring closure ms, 256 pairs | status |
|---|---:|---:|---|
| `pglite-query` | 2.257625 | 2.821083 | exact ordered pairs |
| `native-postgres-query` | 1.1305 | 1.293333 | exact ordered pairs |
| `pglite-pg_ivm` | - | - | unsupported recursive view |
| `native-postgres-pg_ivm` | - | - | unsupported recursive view |

The two SQL arms emitted the same chain checksum
`27983d72a697692be5be47b93b6f9d0f7d38ae7e9aa536bb2d88a4b52a2a9463`
and ring checksum
`e8fe89894ebcd498cd47f104eabef525bc5baf0f0a3970d667461e899c95e267`.

The v6 shared runner has no native Differential Dataflow library arm in its
current engine array. The v7 names `dbsp-kernel`, `dbsp-generated`, and
`dbsp-sqlite` identify the existing local Rust RAM evaluator, generated-
constructor Rust evaluator, and generated-SQL SQLite fixed point described by
that lab. This report does not treat those labels as a native Feldera DBSP
library measurement.

## Tests and current executed coverage

| check | current result |
|---|---|
| v6 graph generator and exact ordered-set oracle | 3 deterministic Node tests passed |
| v7 chain/ring generator and exact ordered-pair oracle | 3 deterministic Node tests passed |
| pg_ivm unsupported status output | deterministic shell test passed for native and PGlite labels |
| JavaScript syntax | both new adapters passed `node --check` |
| shell syntax | shared runner, report, chart, lifecycle, wrapper, unsupported adapter, and v7 runner passed `bash -n` |
| shared smoke | five existing compatible arms plus two ordinary SQL arms completed at `2x200`; both pg_ivm arms recorded unsupported |
| shared scale | 20 numeric rows; 6 SQL exact-set cells; 1 explicit existing-arm timeout; 6 pg_ivm unsupported status rows |
| v7 smoke | 16 numeric chain/ring rows across 8 arms; 2 pg_ivm unsupported records |
| default opt-in isolation | a default-mode filtered `swipl-pure` smoke emitted no PostgreSQL row |
| generated PNG inspection | four 900 by 560 PNG files parsed; retract chart visually inspected |
| existing CI workflows | unchanged; no CI coverage added, changed, or removed |

## Receipt hashes

```text
e70b3f20a54ca212e9fd867812736ddcf9a05436ab1949d343b80f9907ae308a  v6/sprefa-store/bench/results/postgres-shared/results.csv
b2ed0bb5f48c00cf2a1aa64d770954dca45caecd3101cf746cc9213cf3081ddf  v6/sprefa-store/bench/results/postgres-shared/adapter-status.tsv
5b5fde64ec37c992629fb8323ecd584be9e120efdb54b4ffe32eed75f2b15c15  prior smoke.jsonl
27ad9db78ee9fad82971967dadbf830a6878235298cd48e5f0d7d06f5f2a4486  prior scale.jsonl
d435ab1e38f99a269cdfd2d7879eced3e26686cc72f30edb16767a2f058ba32a  prior scale-summary.tsv
6b0ccbd448f6ddd84d9e07687e9e8b02e9d9aa29b865ae3297dc87dbb1369b67  prior crossover-smoke.jsonl
04962d3328928887a82f277a36a7d39de8dfe77c2852db66c9fc4853024992fa  prior crossover-full.jsonl
3ff5390027df96fd8ec693ce8d25c4b50ee6e8ec5be9622e9243f4a30620b873  prior crossover-diagnostics.jsonl
4135ef5a572fbe07b560c88602f52c2f378d492b2ca05dc846507134fa5cb733  prior crossover-summary.tsv
b57c539240d59625c07cbc979edca55f02d82ed824f07eecc571fa2d3bf6ebb1  prior crossover-family-summary.tsv
499a63ffbe7aa13919e4ae87ce4f58c01ddf34959d9a0995c2f667c16a7ec798  prior crossover-heatmaps.svg
```

The prior receipt hashes are byte-identical to the values recorded in the two
earlier task reports.

## Commits

| commit | content |
|---|---|
| `787e02e77` | contract mapping and pre-implementation inventory |
| `1e6ab7021` | opt-in shared harness adapters, exact oracle, status pipeline, report corrections |
| `f4ed83f10` | opt-in 120-second per-cell process-group bound |
| `9283959c1` | opt-in v7 ordinary PostgreSQL/PGlite arms and exact closure oracle |
| `dbe0d86c0` | shared CSV/status/chart receipts and v7 smoke receipt |

No compiler kernel, graph semantics, binding, rule evaluation, macrotime or
comptime boundary, production service, global setting, primary checkout,
merge, or push was changed.
