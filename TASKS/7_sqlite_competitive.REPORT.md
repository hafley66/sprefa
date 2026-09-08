# Competitive SQLite-native IVM

Expanded authorization: `651913433`, brief `TASKS/7_sqlite_competitive.BRIEF.md`.

Combined baseline: PASS. Seven arms, two repetitions, 13 aggregate_churn states
per arm with exact shared input/output hashes. PostgreSQL 18.6 and pg_ivm use the
prior task-local installation; DD and Take 1 use existing task-local binaries.
No prior-lane processes or databases are touched. Node dependencies are read
through a symlink in this worktree.

| Arm | Median cumulative mutation + query ms |
|---|---:|
| DD | 0.467 |
| SWI | 0.521 |
| SQLite full query | 2.544 |
| Take 2 | 6.301 |
| Take 1 | 7.398 |
| PG full query | 6.366 |
| pg_ivm | 18.044 |

This is the 24-row, batch-3, fanout-4 circuit, not the batch1000 workload.
DD/SWI are volatile. SWI uses full predicate recomputation for aggregate_churn.
SQL arms retain durable WAL/FULL or PostgreSQL fsync/synchronous_commit settings.

```sh
IVM_POSTGRES_PREFIX=/Users/chrishafley/projects/sprefa/.boop-worktrees/feature/postgres-pglite-ivm/v6/labs/exec_shootout/postgres_pglite_ivm/.work/postgres-18.6 bash v6/labs/exec_shootout/postgres_pglite_ivm/13_crossover_run.sh circuits /tmp/sprefa-sqlite-competitive/combined-baseline.jsonl --circuits aggregate_churn --arms query,pg_ivm,dd,swi-circuit,sqlite-query,sqlite-plugin-delta,sqlite-native-take2 --circuit-dd-bin /private/tmp/sqlite-ivm-astra-target/release/examples/circuit_dd --sqlite-extension /private/tmp/sqlite-ivm-astra-target/release/libsqlite_ivm.dylib --take2-extension /tmp/sprefa-sqlite-native-take2/gate-u9mxe9w9/take2.dylib --repetitions 2
```

Exit code 0. Full receipt copied to `43_sqlite_competitive/receipts/`.
CI coverage adds actual Take 2 aggregate_churn execution in the shared circuit
runner. Existing engine implementations and Take 1/Take 2 extension sources
remain preserved. Profiling and batching are the next steps.
