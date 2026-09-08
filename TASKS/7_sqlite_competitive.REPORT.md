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

## Prepared statements, batching and source views

Implemented a separate competitive extension, leaving `42_sqlite_native_take2`
source and receipts untouched. It adds a bounded 32-statement cache, signed delta
consolidation and explicit SQL batch/flush operations. The source-view variant
reads indexed ordinary source tables instead of duplicating their row images.
Missing flush, reads inside a batch and missing writer configuration fail closed.

The profiling wrapper measured 15007 nested write preparations for the four
batch1000 mutations, with 102.2 ms in prepare out of 191.8 ms total. A 32-entry
cache reduced preparations to 4 and total to 79.7 ms in that profiling run.
Invariant-read preparations are outside the wrapper, so this attribution is
partial. Persistent delta consolidation then measured 68.8 ms. Source views and
per-source index pushdown measured 23.867 ms in the final focused profile.

Retained failure: attempting DROP TABLE inside xUpdate returned SQLITE_LOCKED.
The repair uses separate source views and ordinary transactional DELETE to empty
unused images. A UNION ALL source view scanned 12108 rows during dimension update;
separate source views reduced this to 109 support-validation scan steps.

Paired 12000-row/batch1000/fanout200 medians, two repetitions per budget:

| Arm | Constrained ms | Roomy ms |
|---|---:|---:|
| DD, volatile | 1.119 | 1.102 |
| PG full query | 21.300 | 20.973 |
| pg_ivm | 22.329 | 20.829 |
| Take 1 | 25.982 | 25.175 |
| Competitive source views | 28.962 | 27.229 |
| Competitive shadow-image batch | 58.770 | 58.430 |
| Preserved Take 2 | 190.936 | 189.073 |

All cells match the same exact input/output oracle. SQL transaction durability
is unchanged. The explicit batch exposes output after flush and before COMMIT;
earlier maintained reads fail. Setup and mutation costs remain separate.

```sh
python3 v6/labs/exec_shootout/postgres_pglite_ivm/43_sqlite_competitive/5_gate.py
# Profiling arguments: extension fixture fresh-db cache batch source-views
python3 v6/labs/exec_shootout/postgres_pglite_ivm/43_sqlite_competitive/2_profile.py /tmp/sprefa-sqlite-competitive/views3.dylib /tmp/sprefa-sqlite-competitive/batch1000.json /tmp/sprefa-sqlite-competitive/profile-views3.db 1 1 1
```

The paired command is retained verbatim in `receipts/sourceview-paired.jsonl`
run-metadata, including engine hashes and existing task-local binary paths.
The finite option ledger is `43_sqlite_competitive/0_README.md`. Session/preupdate
capture is not admitted without an exclusive hook-ownership proof; SQLite's
session API documents undefined behavior with an independently installed
preupdate hook. No hook was replaced and no custom SQLite patch was needed for
the explicit boundary. Broader circuit families are the next implementation step.
