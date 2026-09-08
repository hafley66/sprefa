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

## Shared circuit expansion

Added scalar filter/projection, bag joins, self chains, three-way chains,
semijoin, antijoin and DRed cyclic reachability in the competitive extension.
SQLite parses scalar expressions in an isolated plan connection. Relational
lowering stays in fixed C/SQL templates; no OpenIVM compiler port or DL7 changes.
Commit `5b4264d7a` preserves the preceding batch/source-view milestone.

Current tests: 13 boundary + 6 original semantic + 9 batch + 9 source-view batch
+ 10 circuit + 10 source-view circuit tests. Shared gates validate 171 semantic
states and 104 circuit states per competitive arm. CI execution coverage adds
these circuit tests and shared cases; existing engine implementations are intact.

Failures retained: the prior competitive binary rejects new circuit modes in
14 focused red runs. The first broadened gate exited 1 because its final receipt
assertion expected one 104-state row; the runner emits eight 13-state rows. All
executed test commands had exited 0. The repaired assertion requires eight exact
paired records, each with both competitive arms and no exclusions.
Green receipt: `/tmp/sprefa-sqlite-competitive/gate-11woyx03/receipt.json`, exit 0,
copied to `receipts/circuits-gate-green.json`. Source and log hashes are preserved.

Paired small-circuit medians, 24 rows/batch3/fanout4, two repetitions, constrained
budget. Values are cumulative mutation + query ms over 13 transitions:

| Circuit | DD volatile | PG full | pg_ivm | SQLite full | Competitive source views |
|---|---:|---:|---:|---:|---:|
| pipeline | 1.149 | 28.544 | 222.557 | 10.518 | 4.373 |
| join | 0.560 | 23.150 | 27.746 | 4.626 | 4.157 |
| self_join | 0.627 | 20.938 | 39.348 | 3.002 | 6.747 |
| chain | 0.798 | 19.315 | 92.651 | 2.971 | 10.992 |
| semijoin | 1.292 | 9.156 | 49.697 | 2.906 | 5.573 |
| antijoin | 0.574 | 9.331 | unsupported | 4.856 | 4.716 |
| reach_cycle | 1.221 | 24.503 | unsupported | 2.196 | 12.067 |
| aggregate_churn | 1.424 | 9.850 | 70.435 | 1.879 | 4.667 |

Each admitted arm validates exact shared input and output hashes. SWI, shadow
batch, Take 1 and Take 2 records are also retained; unsupported cells are explicit.
SQL arms preserve durable settings; DD/SWI retain volatile labels. This bounded
run has startup/host variability, with no throughput extrapolation.

Command: same `13_crossover_run.sh circuits` as above, adding
`--circuits pipeline,join,self_join,chain,semijoin,antijoin,reach_cycle,aggregate_churn`,
both competitive arms, `--competitive-extension /tmp/sprefa-sqlite-competitive/circuits2.dylib`,
`--warmups 0 --repetitions 2`, and `IVM_BUDGETS=constrained`. The complete command,
binary hashes and timings are in `receipts/circuits-paired.jsonl`; shell exit 0.

Remaining gaps: time/frontier semantics; remaining shared circuit catalog modes;
source-view teardown; automatic statement-end batching; session/preupdate capture
under exclusive hook ownership; custom SQLite variant. The current explicit SQL
boundary needs no custom SQLite patch. The option ledger distinguishes measured
paths, observed restrictions and unimplemented candidates.

## Teardown and bounded sweep

Circuit commit: `7eb0376fa`. A focused teardown red test exposed leftover
per-source views/indexes, triggers and DRed cone storage. DROP now removes owned
objects transactionally; rollback restores exact schema and output. Current
circuit tests are 11 per layout, bringing test methods to 59. Source-view teardown
is covered by execution in the reproducible gate.

Three additional shared Bash cells passed, seven arms and two repetitions each,
constrained budget. All measured cells match the exact input/output oracle.

| Rows / batch / fanout | DD volatile | pg_ivm | Take 1 | Take 2 | Competitive source views |
|---|---:|---:|---:|---:|---:|
| 400 / 10 / 10 | 0.137 | 6.832 | 1.724 | 2.854 | 1.834 |
| 12000 / 10 / 200 | 0.302 | 8.842 | 2.068 | 4.552 | 3.025 |
| 12000 / 100 / 200 | 0.529 | 10.556 | 6.992 | 23.877 | 7.708 |

Values are median cumulative mutation + query ms. Setup, RSS scope, durable SQL
settings, DB/WAL sizes and full-query/shadow-batch results remain in each JSONL.
`receipts/sweep-receipt.json` preserves exact commands and all three exit codes 0.
The swept binary is pinned by hash in each run's metadata, before teardown edits.

## Core circuit catalog and cache experiment

Teardown/sweep commit: `3478a9bbb`. Added DISTINCT support, fanout/fanin and diamond
bag union lowering. All 11 core circuit families now run in both competitive
layouts. Current gate: 65 test methods, 171 semantic states plus 143 circuit states
per arm. `/tmp/sprefa-sqlite-competitive/gate-c9hf310n/receipt.json` exits 0.
Three focused unsupported-mode red tests precede the implementation; logs remain.

Two-repetition small shared-circuit medians (mutation + query ms):

| Circuit | DD volatile | pg_ivm | SQLite full | Competitive source views |
|---|---:|---:|---:|---:|
| distinct | 0.567 | 15.454 | 2.223 | 3.942 |
| fanout_fanin | 0.285 | unsupported | 1.759 | 7.798 |
| diamond | 0.470 | unsupported | 1.894 | 6.892 |

`receipts/unions-paired.jsonl` retains the complete shared Bash command, hashes,
other arms, unsupported cells and memory/disk telemetry; exit 0. Exact oracle
checks precede every admitted timing record.

Six focused batch1000 profiles exercise SQLite session-local pager targets of
1024/8192/32768 KiB. Median totals: 28.440/29.046/26.834 ms, two runs per target.
The profile transport accepts this target as its last argument; each run retains
extension hash, target and exact SQL oracle checks. `pager-receipt.json` records
six exit codes 0. This uses SQLite's pager and the 32-statement extension cache;
no copied source relation is held by Python or C. Pager targets are not hard RSS
limits. The shared benchmark retains its existing memory guards and cache target.
