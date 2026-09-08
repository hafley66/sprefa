# Z-set / IVM head-to-head: feasibility lab

Same computation in every engine: reachability from roots {0,1} over a generated
layered graph, then **retract root 0** and recount the survivor set. The PostgreSQL-family
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
| swi-incr | 402 | 667 | 200 | 2 | 1 | 0 | 13.36 | N/A | N/A | N/A |
| swi-incr | 12002 | 22667 | 2000 | 81 | 85 | 0 | 49.19 | N/A | N/A | N/A |
| swi-incr | 160002 | 306667 | 20000 | 1214 | 1300 | 0 | 675.86 | N/A | N/A | N/A |
| differential-dataflow | 402 | 667 | 200 | 1.117 | 0.247 | 200 | 3.2 | N/A | N/A | N/A |
| differential-dataflow | 12002 | 22667 | 2000 | 7.143 | 2.408 | 2000 | 9.1 | N/A | N/A | N/A |
| differential-dataflow | 160002 | 306667 | 20000 | 102.298 | 27.838 | 20000 | 112.8 | N/A | N/A | N/A |
| sqlite-count | 402 | 667 | 200 | 3.902 | 0.658 | 23 | 6.5 | 0.11 | 0.29 | 0.07 |
| sqlite-count | 12002 | 22667 | 2000 | 22.728 | 5.761 | 23 | 16.3 | 0.21 | 1.34 | 0.79 |
| sqlite-count | 160002 | 306667 | 20000 | 310.486 | 54.676 | 23 | 115.4 | 1.22 | 12.93 | 10.25 |
| sqlite-count-scc | 402 | 667 | 200 | 5.021 | 0.749 | 15 | 6.6 | 0.11 | 0.29 | 0.07 |
| sqlite-count-scc | 12002 | 22667 | 2000 | 22.455 | 21.201 | 39 | 15.1 | 0.21 | 1.34 | 0.79 |
| sqlite-count-scc | 160002 | 306667 | 20000 | 312.897 | 292.036 | 51 | 121.3 | 1.22 | 14.32 | 10.25 |
| sqlite-dred-loop | 402 | 667 | 200 | 5.242 | 0.918 | 25 | 6.8 | 0.11 | 0.29 | 0.07 |
| sqlite-dred-loop | 12002 | 22667 | 2000 | 22.587 | 21.515 | 53 | 16.0 | 0.21 | 1.34 | 0.79 |
| sqlite-dred-loop | 160002 | 306667 | 20000 | 310.855 | 301.057 | 67 | 121.5 | 1.22 | 14.32 | 10.25 |
| sqlite-dred-cte | 402 | 667 | 200 | 4.494 | 0.780 | 6 | 7.2 | 0.11 | 0.29 | 0.07 |
| sqlite-dred-cte | 12002 | 22667 | 2000 | 22.386 | 29.374 | 6 | 16.1 | 0.21 | 1.53 | 0.79 |
| sqlite-dred-cte | 160002 | 306667 | 20000 | 317.550 | 441.133 | 6 | 132.6 | 1.22 | 18.20 | 10.25 |
| sqlite-signed-delta-v2 | 402 | 667 | 200 | 4.458 | 0.407 | 3 | 6.7 | 0.11 | 0.29 | 0.07 |
| sqlite-signed-delta-v2 | 12002 | 22667 | 2000 | 22.313 | 13.826 | 3 | 15.2 | 0.21 | 1.35 | 0.79 |
| sqlite-signed-delta-v2 | 160002 | 306667 | 20000 | 315.209 | 188.954 | 3 | 130.3 | 1.22 | 15.15 | 10.25 |
| tsv2-runtime | 402 | 667 | 200 | 16.028 | 3.916 | 92 | 146.4 | N/A | N/A | 0 |
| sprefa-engine-rs | 402 | 667 | 200 | 6.922 | 1.163 | 52 | 9.3 | N/A | N/A | 0 |
| sprefa-engine-rs | 12002 | 22667 | 2000 | 165.331 | 56.699 | 76 | 64.3 | N/A | N/A | 0 |
| sprefa-engine-rs | 160002 | 306667 | 20000 | 2222.133 | 1033.627 | 88 | 496.4 | N/A | N/A | 0 |
| pglite-query | 402 | 667 | 200 | 12.317 | 2.755 | N/A | 1055.2 | N/A | N/A | 38.117 |
| pglite-query | 12002 | 22667 | 2000 | 66.581 | 29.356 | N/A | 1108.0 | N/A | N/A | 39.274 |
| pglite-query | 160002 | 306667 | 20000 | 790.827 | 459.353 | N/A | 1188.3 | N/A | N/A | 70.274 |
| native-postgres-query | 402 | 667 | 200 | 7.206 | 2.295 | N/A | 109.9 | N/A | N/A | 7.412 |
| native-postgres-query | 12002 | 22667 | 2000 | 41.919 | 22.325 | N/A | 185.0 | N/A | N/A | 8.568 |
| native-postgres-query | 160002 | 306667 | 20000 | 599.926 | 255.202 | N/A | 472.5 | N/A | N/A | 23.568 |

## Adapter status and measurement scope

| engine | scale | status | semantics or reason | memory scope | memory-limit scope |
|---|---|---|---|---|---|
| swi-incr | 2x200 | ok | SWI incremental tabling; setup and retract end after table materialization and count; exact ordered initial set matches the generated node range and exact incremental survivor set matches cold table recomputation | process RSS sampled with ps after both phases | DL_MEMCAP_MB is passed by the harness but unenforced by this adapter |
| swi-incr | 6x2000 | ok | SWI incremental tabling; setup and retract end after table materialization and count; exact ordered initial set matches the generated node range and exact incremental survivor set matches cold table recomputation | process RSS sampled with ps after both phases | DL_MEMCAP_MB is passed by the harness but unenforced by this adapter |
| swi-incr | 8x20000 | ok | SWI incremental tabling; setup and retract end after table materialization and count; exact ordered initial set matches the generated node range and exact incremental survivor set matches cold table recomputation | process RSS sampled with ps after both phases | DL_MEMCAP_MB is passed by the harness but unenforced by this adapter |
| differential-dataflow | 2x200 | ok | differential-dataflow 0.25 with timely 0.31; shared layered graph; setup and retract end after fixed-point materialization and count; exact ordered sets match BFS oracle | process peak RSS from getrusage including untimed oracle allocations | DL_MEMCAP_MB=4096 caps live Rust allocations through CappedAlloc (0 disables); RLIMIT_AS and RLIMIT_DATA are also requested best-effort; total process RSS is not capped |
| differential-dataflow | 6x2000 | ok | differential-dataflow 0.25 with timely 0.31; shared layered graph; setup and retract end after fixed-point materialization and count; exact ordered sets match BFS oracle | process peak RSS from getrusage including untimed oracle allocations | DL_MEMCAP_MB=4096 caps live Rust allocations through CappedAlloc (0 disables); RLIMIT_AS and RLIMIT_DATA are also requested best-effort; total process RSS is not capped |
| differential-dataflow | 8x20000 | ok | differential-dataflow 0.25 with timely 0.31; shared layered graph; setup and retract end after fixed-point materialization and count; exact ordered sets match BFS oracle | process peak RSS from getrusage including untimed oracle allocations | DL_MEMCAP_MB=4096 caps live Rust allocations through CappedAlloc (0 disables); RLIMIT_AS and RLIMIT_DATA are also requested best-effort; total process RSS is not capped |
| sqlite-count | 2x200 | ok | native RelStore specialized incremental cascade; materialize and count inside clocks; exact initial and survivor sets validated outside clocks; tagged-node input blake3=d21ac679d2ae1b63; survivor blake3=871dc3852156956e | process peak RSS includes setup and oracle; separate Rust allocation and SQLite C-heap high-water columns | DL_MEMCAP_MB=4096 caps live Rust allocations; SQLite C heap and total RSS are unenforced |
| sqlite-count | 6x2000 | ok | native RelStore specialized incremental cascade; materialize and count inside clocks; exact initial and survivor sets validated outside clocks; tagged-node input blake3=6035d0f3b9cf2bb5; survivor blake3=979516e118581f07 | process peak RSS includes setup and oracle; separate Rust allocation and SQLite C-heap high-water columns | DL_MEMCAP_MB=4096 caps live Rust allocations; SQLite C heap and total RSS are unenforced |
| sqlite-count | 8x20000 | ok | native RelStore specialized incremental cascade; materialize and count inside clocks; exact initial and survivor sets validated outside clocks; tagged-node input blake3=fea9e9d65d21ef13; survivor blake3=239d2120ef6ba1c3 | process peak RSS includes setup and oracle; separate Rust allocation and SQLite C-heap high-water columns | DL_MEMCAP_MB=4096 caps live Rust allocations; SQLite C heap and total RSS are unenforced |
| sqlite-count-scc | 2x200 | ok | native RelStore specialized incremental cascade; materialize and count inside clocks; exact initial and survivor sets validated outside clocks; tagged-node input blake3=d21ac679d2ae1b63; survivor blake3=871dc3852156956e | process peak RSS includes setup and oracle; separate Rust allocation and SQLite C-heap high-water columns | DL_MEMCAP_MB=4096 caps live Rust allocations; SQLite C heap and total RSS are unenforced |
| sqlite-count-scc | 6x2000 | ok | native RelStore specialized incremental cascade; materialize and count inside clocks; exact initial and survivor sets validated outside clocks; tagged-node input blake3=6035d0f3b9cf2bb5; survivor blake3=979516e118581f07 | process peak RSS includes setup and oracle; separate Rust allocation and SQLite C-heap high-water columns | DL_MEMCAP_MB=4096 caps live Rust allocations; SQLite C heap and total RSS are unenforced |
| sqlite-count-scc | 8x20000 | ok | native RelStore specialized incremental cascade; materialize and count inside clocks; exact initial and survivor sets validated outside clocks; tagged-node input blake3=fea9e9d65d21ef13; survivor blake3=239d2120ef6ba1c3 | process peak RSS includes setup and oracle; separate Rust allocation and SQLite C-heap high-water columns | DL_MEMCAP_MB=4096 caps live Rust allocations; SQLite C heap and total RSS are unenforced |
| sqlite-dred-loop | 2x200 | ok | native RelStore specialized incremental cascade; materialize and count inside clocks; exact initial and survivor sets validated outside clocks; tagged-node input blake3=d21ac679d2ae1b63; survivor blake3=871dc3852156956e | process peak RSS includes setup and oracle; separate Rust allocation and SQLite C-heap high-water columns | DL_MEMCAP_MB=4096 caps live Rust allocations; SQLite C heap and total RSS are unenforced |
| sqlite-dred-loop | 6x2000 | ok | native RelStore specialized incremental cascade; materialize and count inside clocks; exact initial and survivor sets validated outside clocks; tagged-node input blake3=6035d0f3b9cf2bb5; survivor blake3=979516e118581f07 | process peak RSS includes setup and oracle; separate Rust allocation and SQLite C-heap high-water columns | DL_MEMCAP_MB=4096 caps live Rust allocations; SQLite C heap and total RSS are unenforced |
| sqlite-dred-loop | 8x20000 | ok | native RelStore specialized incremental cascade; materialize and count inside clocks; exact initial and survivor sets validated outside clocks; tagged-node input blake3=fea9e9d65d21ef13; survivor blake3=239d2120ef6ba1c3 | process peak RSS includes setup and oracle; separate Rust allocation and SQLite C-heap high-water columns | DL_MEMCAP_MB=4096 caps live Rust allocations; SQLite C heap and total RSS are unenforced |
| sqlite-dred-cte | 2x200 | ok | native RelStore specialized incremental cascade; materialize and count inside clocks; exact initial and survivor sets validated outside clocks; tagged-node input blake3=d21ac679d2ae1b63; survivor blake3=871dc3852156956e | process peak RSS includes setup and oracle; separate Rust allocation and SQLite C-heap high-water columns | DL_MEMCAP_MB=4096 caps live Rust allocations; SQLite C heap and total RSS are unenforced |
| sqlite-dred-cte | 6x2000 | ok | native RelStore specialized incremental cascade; materialize and count inside clocks; exact initial and survivor sets validated outside clocks; tagged-node input blake3=6035d0f3b9cf2bb5; survivor blake3=979516e118581f07 | process peak RSS includes setup and oracle; separate Rust allocation and SQLite C-heap high-water columns | DL_MEMCAP_MB=4096 caps live Rust allocations; SQLite C heap and total RSS are unenforced |
| sqlite-dred-cte | 8x20000 | ok | native RelStore specialized incremental cascade; materialize and count inside clocks; exact initial and survivor sets validated outside clocks; tagged-node input blake3=fea9e9d65d21ef13; survivor blake3=239d2120ef6ba1c3 | process peak RSS includes setup and oracle; separate Rust allocation and SQLite C-heap high-water columns | DL_MEMCAP_MB=4096 caps live Rust allocations; SQLite C heap and total RSS are unenforced |
| sqlite-signed-delta-v2 | 2x200 | ok | native RelStore specialized incremental cascade; materialize and count inside clocks; exact initial and survivor sets validated outside clocks; tagged-node input blake3=d21ac679d2ae1b63; survivor blake3=871dc3852156956e | process peak RSS includes setup and oracle; separate Rust allocation and SQLite C-heap high-water columns | DL_MEMCAP_MB=4096 caps live Rust allocations; SQLite C heap and total RSS are unenforced |
| sqlite-signed-delta-v2 | 6x2000 | ok | native RelStore specialized incremental cascade; materialize and count inside clocks; exact initial and survivor sets validated outside clocks; tagged-node input blake3=6035d0f3b9cf2bb5; survivor blake3=979516e118581f07 | process peak RSS includes setup and oracle; separate Rust allocation and SQLite C-heap high-water columns | DL_MEMCAP_MB=4096 caps live Rust allocations; SQLite C heap and total RSS are unenforced |
| sqlite-signed-delta-v2 | 8x20000 | ok | native RelStore specialized incremental cascade; materialize and count inside clocks; exact initial and survivor sets validated outside clocks; tagged-node input blake3=fea9e9d65d21ef13; survivor blake3=239d2120ef6ba1c3 | process peak RSS includes setup and oracle; separate Rust allocation and SQLite C-heap high-water columns | DL_MEMCAP_MB=4096 caps live Rust allocations; SQLite C heap and total RSS are unenforced |
| tsv2-runtime | 2x200 | ok | compile_dl6 emitted program.tick through IncrementalRuntime; emitted recursive DRed plan; exact input edges and before/after sets match shared BFS outside clocks; count inside clocks | Node process RSS sampled after validation; in-memory libSQL store | DL_MEMCAP_MB limits Node old-space only; SQLite C heap and total RSS unenforced |
| tsv2-runtime | 6x2000 | error | process exited with status 1; see logs/tsv2-runtime-6x2000.log | unavailable | unknown |
| tsv2-runtime | 8x20000 | error | process exited with status 1; see logs/tsv2-runtime-8x20000.log | unavailable | unknown |
| sprefa-engine-rs | 2x200 | ok | compile_dl6 emit_rust program through drive_tick_transacted and GenProgram::run_tick; exact input edges and before/after sets match shared BFS outside clocks; count inside clocks | child-process peak RSS from time; in-memory rusqlite store | DL_MEMCAP_MB requested but unenforced for this adapter |
| sprefa-engine-rs | 6x2000 | ok | compile_dl6 emit_rust program through drive_tick_transacted and GenProgram::run_tick; exact input edges and before/after sets match shared BFS outside clocks; count inside clocks | child-process peak RSS from time; in-memory rusqlite store | DL_MEMCAP_MB requested but unenforced for this adapter |
| sprefa-engine-rs | 8x20000 | ok | compile_dl6 emit_rust program through drive_tick_transacted and GenProgram::run_tick; exact input edges and before/after sets match shared BFS outside clocks; count inside clocks | child-process peak RSS from time; in-memory rusqlite store | DL_MEMCAP_MB requested but unenforced for this adapter |
| pglite-query | 2x200 | ok | ordinary recursive SQL full recomputation; timed phases end after materialization and count; exact before=ada7ea43e998e3ba1357b4270279f2976e2659c6e3dd768d0fbfc6a15cd11314 after=d290b0a93c2138767799bffe561de2bad1a45c0ffc8210a3be393f16eb7bc305; untimed transfer_ms before=0.993 after=0.496; untimed checksum_validation_ms before=1.191 after=0.096; PostgreSQL 18.3 | sampled RSS of the Node process containing PGlite WASM | DL_MEMCAP_MB=4096 limits Node old-space only; total process RSS is unenforced |
| pglite-query | 6x2000 | ok | ordinary recursive SQL full recomputation; timed phases end after materialization and count; exact before=5359ff08f251a47ab81d104beeff9633f95d50c30ea2ff1db9d40617983ca17b after=e640e84f06cd3d340d290625432f23bf9cf4b9df50d6e4ae1ab7a73f216df46b; untimed transfer_ms before=16.711 after=12.857; untimed checksum_validation_ms before=4.369 after=1.985; PostgreSQL 18.3 | sampled RSS of the Node process containing PGlite WASM | DL_MEMCAP_MB=4096 limits Node old-space only; total process RSS is unenforced |
| pglite-query | 8x20000 | ok | ordinary recursive SQL full recomputation; timed phases end after materialization and count; exact before=d5b111c00b9de6c6acc308845af36202735166f81b10ef25547078a8e03fcd77 after=ba46515280fa0525997e4d27a81f4d2dfdb3f4445c1e54d0cc969e59cb068bf2; untimed transfer_ms before=216.229 after=170.858; untimed checksum_validation_ms before=29.378 after=25.389; PostgreSQL 18.3 | sampled RSS of the Node process containing PGlite WASM | DL_MEMCAP_MB=4096 limits Node old-space only; total process RSS is unenforced |
| native-postgres-query | 2x200 | ok | ordinary recursive SQL full recomputation; timed phases end after materialization and count; exact before=ada7ea43e998e3ba1357b4270279f2976e2659c6e3dd768d0fbfc6a15cd11314 after=d290b0a93c2138767799bffe561de2bad1a45c0ffc8210a3be393f16eb7bc305; untimed transfer_ms before=0.787 after=0.182; untimed checksum_validation_ms before=0.548 after=0.100; PostgreSQL 18.6 | sampled sum of Node client and disposable PostgreSQL process-tree RSS; shared mappings may be counted more than once | DL_MEMCAP_MB=4096 is unenforced for total PostgreSQL memory |
| native-postgres-query | 6x2000 | ok | ordinary recursive SQL full recomputation; timed phases end after materialization and count; exact before=5359ff08f251a47ab81d104beeff9633f95d50c30ea2ff1db9d40617983ca17b after=e640e84f06cd3d340d290625432f23bf9cf4b9df50d6e4ae1ab7a73f216df46b; untimed transfer_ms before=6.935 after=2.633; untimed checksum_validation_ms before=3.903 after=2.466; PostgreSQL 18.6 | sampled sum of Node client and disposable PostgreSQL process-tree RSS; shared mappings may be counted more than once | DL_MEMCAP_MB=4096 is unenforced for total PostgreSQL memory |
| native-postgres-query | 8x20000 | ok | ordinary recursive SQL full recomputation; timed phases end after materialization and count; exact before=d5b111c00b9de6c6acc308845af36202735166f81b10ef25547078a8e03fcd77 after=ba46515280fa0525997e4d27a81f4d2dfdb3f4445c1e54d0cc969e59cb068bf2; untimed transfer_ms before=54.493 after=44.609; untimed checksum_validation_ms before=30.110 after=24.371; PostgreSQL 18.6 | sampled sum of Node client and disposable PostgreSQL process-tree RSS; shared mappings may be counted more than once | DL_MEMCAP_MB=4096 is unenforced for total PostgreSQL memory |
| pglite-pg_ivm | 2x200 | unsupported | pg_ivm rejects recursive WITH RECURSIVE view definitions | no process started | not applicable |
| pglite-pg_ivm | 6x2000 | unsupported | pg_ivm rejects recursive WITH RECURSIVE view definitions | no process started | not applicable |
| pglite-pg_ivm | 8x20000 | unsupported | pg_ivm rejects recursive WITH RECURSIVE view definitions | no process started | not applicable |
| native-postgres-pg_ivm | 2x200 | unsupported | pg_ivm rejects recursive WITH RECURSIVE view definitions | no process started | not applicable |
| native-postgres-pg_ivm | 6x2000 | unsupported | pg_ivm rejects recursive WITH RECURSIVE view definitions | no process started | not applicable |
| native-postgres-pg_ivm | 8x20000 | unsupported | pg_ivm rejects recursive WITH RECURSIVE view definitions | no process started | not applicable |

## Takeaways (derived)

- Largest scale reached by a numeric run: 160002 nodes.
- No numeric arm emitted a WALL row at these scales.
