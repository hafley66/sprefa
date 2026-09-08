# Z-set / IVM head-to-head: feasibility lab

Same computation in every engine: reachability from roots {0,1} over a generated
DAG, then **retract root 0** and recount the survivor set. The PostgreSQL-family
numeric arms compare complete ordered results with an independent BFS oracle
before emitting CSV. Only the root retraction and survivor recount are the
measured operation; setup is reported separately. Requested memory budget:
4096 MB/run. Enforcement and accounting scope are adapter-specific and
recorded below.

Graph back-edge stride: 0 (0 is the layered DAG;
positive values add the existing cyclic workload's child-to-parent edges).
Input-hash gating: 1. When enabled, every numeric
arm must match the shared canonical edge-list SHA-256 in `input-hashes.tsv`.
The tagged store keys are normalized to global node IDs for this comparison.

Ordinary PostgreSQL and PGlite arms execute the recursive query from scratch
after the root deletion. Their timed phase ends after query materialization and
`count(*)`. Ordered full-result transfer, checksum, and exact validation are
recorded in the status receipt and excluded from `setup_ms` and
`retract_ms`. Their rows are full recomputation measurements.

The `differential-dataflow` arm is the native Differential Dataflow 0.25
library over timely 0.31. Its setup and retract phases end after fixed-point
convergence and counting. Complete ordered
initial and survivor sets are checked against an independent BFS outside the
timed phases.

The `swi-incr` arm uses SWI incremental tabling and wall time. Its clock is
reset after initial validation and immediately before root deletion. Both
phases end after table materialization and counting; exact initial-range and
incremental-versus-cold survivor checks run outside the clocks.

The optional `swipl-pure` reference uses CPU time and does not perform an
exact-set oracle check. Keep it outside a wall-time comparison. RSS combines
adapter-specific samples and process peaks, including untimed validation;
the scope receipts describe which processes and limits are included.

Per-cell output and exit codes are retained in `logs/`. Failed processes and
error/timeout/OOM status rows do not contribute numeric measurements. A run
refuses to overwrite existing CSV/status receipts.

When selected, the `sqlite-*` arms call the native store's count, SCC, DRed,
or signed-delta implementation through `perf_report --shared`. The
`tsv2-runtime` and `sprefa-engine-rs` arms execute compiler-emitted programs
through their actual runtime tick methods. These adapters validate complete
initial and survivor sets outside the clocks, and include counting inside.
The older `tsv2_retract.sh` specialized SQL script is not a generated-runtime
arm and is not selected by `SQLITE_SHOOTOUT`.

## Charts

![retract](retract_ms.png)
![setup](setup_ms.png)
![rss](rss_mb.png)
![ops](ops.png)

## Data

| engine | nodes | edges | killed | setup ms | retract ms | ops | RSS MB | host peak MB | SQLite high-water MB | db MB |
|---|---:|---:|---:|---:|---:|---:|---:|---:|---:|---:|
| swi-incr | 402 | 667 | 200 | 1 | 1 | 0 | 13.19 | N/A | N/A | N/A |
| differential-dataflow | 402 | 667 | 200 | 1.209 | 0.261 | 200 | 3.2 | N/A | N/A | N/A |
| sqlite-count | 402 | 667 | 200 | 3.393 | 0.609 | 23 | 5.9 | 0.11 | 0.29 | 0.07 |
| sqlite-count-scc | 402 | 667 | 200 | 2.510 | 0.880 | 15 | 6.4 | 0.11 | 0.29 | 0.07 |
| sqlite-dred-loop | 402 | 667 | 200 | 5.515 | 0.881 | 25 | 6.9 | 0.11 | 0.29 | 0.07 |
| sqlite-dred-cte | 402 | 667 | 200 | 3.252 | 1.010 | 6 | 7.1 | 0.11 | 0.29 | 0.07 |
| sqlite-signed-delta-v2 | 402 | 667 | 200 | 3.273 | 0.384 | 3 | 6.8 | 0.11 | 0.29 | 0.07 |
| tsv2-runtime | 402 | 667 | 200 | 16.062 | 4.107 | 92 | 148.4 | N/A | N/A | 0 |
| sprefa-engine-rs | 402 | 667 | 200 | 6.342 | 1.136 | 52 | 8.9 | N/A | N/A | 0 |
| pglite-query | 402 | 667 | 200 | 12.238 | 2.071 | N/A | 1065.3 | N/A | N/A | 38.117 |
| native-postgres-query | 402 | 667 | 200 | 5.225 | 2.317 | N/A | 141.6 | N/A | N/A | 7.412 |

## Adapter status and measurement scope

| engine | scale | status | semantics or reason | memory scope | memory-limit scope |
|---|---|---|---|---|---|
| swi-incr | 2x200 | ok | SWI incremental tabling; setup and retract end after table materialization and count; exact ordered initial set matches the generated node range and exact incremental survivor set matches cold table recomputation | process RSS sampled with ps after both phases | DL_MEMCAP_MB is passed by the harness but unenforced by this adapter |
| differential-dataflow | 2x200 | ok | differential-dataflow 0.25 with timely 0.31; shared contiguous-node DAG; setup and retract end after fixed-point materialization and count; exact ordered sets match BFS oracle | process peak RSS from getrusage including untimed oracle allocations | DL_MEMCAP_MB=4096 caps live Rust allocations through CappedAlloc (0 disables); RLIMIT_AS and RLIMIT_DATA are also requested best-effort; total process RSS is not capped |
| sqlite-count | 2x200 | ok | native RelStore specialized incremental cascade; materialize and count inside clocks; exact initial and survivor sets validated outside clocks; tagged-node input blake3=d21ac679d2ae1b63; survivor blake3=871dc3852156956e | process peak RSS includes setup and oracle; separate Rust allocation and SQLite C-heap high-water columns | DL_MEMCAP_MB=4096 caps live Rust allocations; SQLite C heap and total RSS are unenforced |
| sqlite-count-scc | 2x200 | ok | native RelStore specialized incremental cascade; materialize and count inside clocks; exact initial and survivor sets validated outside clocks; tagged-node input blake3=d21ac679d2ae1b63; survivor blake3=871dc3852156956e | process peak RSS includes setup and oracle; separate Rust allocation and SQLite C-heap high-water columns | DL_MEMCAP_MB=4096 caps live Rust allocations; SQLite C heap and total RSS are unenforced |
| sqlite-dred-loop | 2x200 | ok | native RelStore specialized incremental cascade; materialize and count inside clocks; exact initial and survivor sets validated outside clocks; tagged-node input blake3=d21ac679d2ae1b63; survivor blake3=871dc3852156956e | process peak RSS includes setup and oracle; separate Rust allocation and SQLite C-heap high-water columns | DL_MEMCAP_MB=4096 caps live Rust allocations; SQLite C heap and total RSS are unenforced |
| sqlite-dred-cte | 2x200 | ok | native RelStore specialized incremental cascade; materialize and count inside clocks; exact initial and survivor sets validated outside clocks; tagged-node input blake3=d21ac679d2ae1b63; survivor blake3=871dc3852156956e | process peak RSS includes setup and oracle; separate Rust allocation and SQLite C-heap high-water columns | DL_MEMCAP_MB=4096 caps live Rust allocations; SQLite C heap and total RSS are unenforced |
| sqlite-signed-delta-v2 | 2x200 | ok | native RelStore specialized incremental cascade; materialize and count inside clocks; exact initial and survivor sets validated outside clocks; tagged-node input blake3=d21ac679d2ae1b63; survivor blake3=871dc3852156956e | process peak RSS includes setup and oracle; separate Rust allocation and SQLite C-heap high-water columns | DL_MEMCAP_MB=4096 caps live Rust allocations; SQLite C heap and total RSS are unenforced |
| tsv2-runtime | 2x200 | ok | compile_dl6 emitted program.tick through IncrementalRuntime; emitted recursive DRed plan; exact input edges and before/after sets match shared BFS outside clocks; count inside clocks | Node process RSS sampled after validation; in-memory libSQL store | DL_MEMCAP_MB limits Node old-space only; SQLite C heap and total RSS unenforced |
| sprefa-engine-rs | 2x200 | ok | compile_dl6 emit_rust program through drive_tick_transacted and GenProgram::run_tick; exact input edges and before/after sets match shared BFS outside clocks; count inside clocks | child-process peak RSS from time; in-memory rusqlite store | DL_MEMCAP_MB requested but unenforced for this adapter |
| pglite-query | 2x200 | ok | ordinary recursive SQL full recomputation; timed phases end after materialization and count; exact before=ada7ea43e998e3ba1357b4270279f2976e2659c6e3dd768d0fbfc6a15cd11314 after=d290b0a93c2138767799bffe561de2bad1a45c0ffc8210a3be393f16eb7bc305; untimed transfer_ms before=0.987 after=0.436; untimed checksum_validation_ms before=0.931 after=0.082; PostgreSQL 18.3 | sampled RSS of the Node process containing PGlite WASM | DL_MEMCAP_MB=4096 limits Node old-space only; total process RSS is unenforced |
| native-postgres-query | 2x200 | ok | ordinary recursive SQL full recomputation; timed phases end after materialization and count; exact before=ada7ea43e998e3ba1357b4270279f2976e2659c6e3dd768d0fbfc6a15cd11314 after=d290b0a93c2138767799bffe561de2bad1a45c0ffc8210a3be393f16eb7bc305; untimed transfer_ms before=0.742 after=0.226; untimed checksum_validation_ms before=0.503 after=0.116; PostgreSQL 18.6 | sampled sum of Node client and disposable PostgreSQL process-tree RSS; shared mappings may be counted more than once | DL_MEMCAP_MB=4096 is unenforced for total PostgreSQL memory |
| pglite-pg_ivm | 2x200 | unsupported | pg_ivm rejects recursive WITH RECURSIVE view definitions | no process started | not applicable |
| native-postgres-pg_ivm | 2x200 | unsupported | pg_ivm rejects recursive WITH RECURSIVE view definitions | no process started | not applicable |

## Takeaways (derived)

- Largest scale reached by a numeric run: 402 nodes.
- No numeric arm emitted a WALL row at these scales.
