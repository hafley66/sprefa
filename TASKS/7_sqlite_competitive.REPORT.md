# Competitive SQLite-native IVM

Current gate exit 0: 147 Python test methods, 2 Rust compiler tests, and the plan
audit, 171 shared semantic states per batch/sourceview/lazy arm, and 143 core
circuit states per batch/sourceview/frontier/lazy arm. All eleven core circuits include actual
incremental maintenance, with negation, cyclic retraction and a separately
labeled finite scalar epoch variant. CI execution coverage is additive.

Reproduce: `python3 v6/labs/exec_shootout/postgres_pglite_ivm/43_sqlite_competitive/5_gate.py`.
Current receipt: `43_sqlite_competitive/receipts/compiler-gate.json`.
Take 1, Take 2, other worktrees and DL7 sources remain preserved. No push or merge.

| Commit | Tested step |
|---|---|
| `651913433` | Expanded authorization brief |
| `0d49d4d46` | Seven-arm exact aggregate baseline |
| `5b4264d7a` | Cached consolidated batches and indexed source views |
| `7eb0376fa` | Shared joins, negation and DRed cyclic retraction |
| `3478a9bbb` | Transactional teardown and three paired sweep cells |
| `ee7ec0fc3` | Eleven core circuits and pager cache measurements |
| `5ec333d19` | Persisted scalar input seals and projection type checks |
| `c4de80359` | Cache schema lifecycle, construction attribution and plan audit |

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
remain preserved. This baseline precedes the batching results below.

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
the explicit boundary. Broader circuit results follow below.

## Shared circuit expansion

Added scalar filter/projection, bag joins, self chains, three-way chains,
semijoin, antijoin and DRed cyclic reachability in the competitive extension.
SQLite parses scalar expressions in an isolated plan connection. Relational
lowering stays in fixed C/SQL templates; no OpenIVM compiler port or DL7 changes.
Commit `5b4264d7a` preserves the preceding batch/source-view milestone.

Circuit milestone tests: 13 boundary + 6 original semantic + 9 batch + 9 source-view batch
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

At this milestone the remaining gaps included time/frontier semantics, remaining
core circuit modes and teardown; later sections record those additions.
Automatic statement-end batching, session/preupdate capture
under exclusive hook ownership; custom SQLite variant. The current explicit SQL
boundary needs no custom SQLite patch. The option ledger distinguishes measured
paths, observed restrictions and unimplemented candidates.

## Teardown and bounded sweep

Circuit commit: `7eb0376fa`. A focused teardown red test exposed leftover
per-source views/indexes, triggers and DRed cone storage. DROP now removes owned
objects transactionally; rollback restores exact schema and output. At this step,
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
layouts. This milestone gate: 65 test methods, 171 semantic states plus 143 circuit states
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

## Finite scalar epoch variant

Core catalog/cache commit: `ee7ec0fc3`. Added `take2_epoch` and the separately
labeled `sqlite-competitive-frontier` arm. SQLite persists a scalar clock and
three input seals. Holding c unsealed blocks completion; a sealed input rejects
late writes. Savepoints, failed commits, reopen and second writers preserve the
contract. An absent module rejects source writes. No DD/kernel changes.

This implements the sequential scalar completion check already present in
`34_circuit_dd.rs` and `33a_dd_host.rs`. Per-record logical times, historical
versions and partial-order antichains remain unsupported. The extension performs
the checks and maintenance; the adapter sends explicit SQL seals.

Current execution coverage: 99 test methods (13 boundary, 6 semantic, 9 batch per
layout, 14 circuit per layout, 17 frontier per layout). Shared gate: 171 semantic
states per batch/source-view arm, and 143 circuit states per batch/source-view/
frontier arm, plus eleven held-input checks. The initial missing-API red receipt,
missing-module red receipt and green logs are retained.
Final gate: `/tmp/sprefa-sqlite-competitive/gate-otdtiuo5/receipt.json`, exit 0,
copied to `receipts/frontier-gate-final.json`. A projection-affinity red test
exposed numeric text/real coercion; the flush now validates SQL expression types
before INTEGER storage affinity. All four circuit/frontier test variants pass.

Paired constrained small-circuit medians, two repetitions, mutation + query ms:

| Circuit | DD volatile | Source views | Scalar-epoch source views |
|---|---:|---:|---:|
| pipeline | 0.548 | 9.158 | 18.666 |
| antijoin | 1.068 | 11.324 | 15.747 |
| reach_cycle | 1.350 | 7.740 | 33.122 |

`receipts/frontier-paired.jsonl` contains the exact Bash command, hashes, PG
records and unsupported cells. Exit 0; twelve finite frontier checks (DD and
SQLite, three circuits, two repetitions) pass. These are observed paired costs
under the existing durable SQL and volatile DD labels; no equal-durability claim.

Remaining scoped gaps: additional semantic catalog operators; arbitrary SELECT
compilation; general recursive rule programs; per-record/partial-order time;
automatic statement-end batching; session capture under exclusive hook ownership;
a custom SQLite variant. Public explicit flush/seal APIs meet the tested boundary
without a custom patch. The finite option ledger records the concrete experiments
and the narrower contracts behind unimplemented alternatives.

## Parent cache and query-plan audit

Added three cache lifecycle tests: index drop/savepoint rollback, second-connection
schema change, and FAIL partial progress/outer rollback. Exact outputs pass.
Index rollback measured six automatic and six fresh prepares; a second schema
cookie measured six fresh prepares. Stable subsequent writes need zero new
prepares. Initial assertions requiring zero fresh prepares across schema changes
failed; that red log is retained.

EXPLAIN with 1000 rows per side and ANALYZE uses the TEXT primary-key index for
key access, `result_result_key` for invariant/zero cleanup, `(side,k)` for state
probes, and the source key index for live views. Dropping the result k index
produces scans; rollback restores indexed plans. An empty-table plan used the
`(side,id)` primary key with only side bound; its initial assertion failed.

Two-repetition batch1000 row-event profile medians, ms:

| Cache | Total | Delta SQL construction | Write prepare/cache lookup | Write step |
|---|---:|---:|---:|---:|
| Off | 211.286 | 4.029 | 109.308 | 45.030 |
| On | 83.071 | 3.594 | 1.097 | 36.642 |

Both execute 2517656 measured write VM steps. Construction covers contribute()
term SQL; invariant/PRAGMA reads remain outside these write counters. No
row-trigger floor is inferred. Three alternating cached profile pairs measured
ANALYZE off/on at 83.095/142.986 ms, with 2517656/704428 write VM steps. ANALYZE
is not enabled in the shared arm by this step. All exact SQL oracle checks pass.

Current gate: 102 test methods plus plan audit, existing 171/143 shared state checks
unchanged. `/tmp/sprefa-sqlite-competitive/gate-fo4wegz3/receipt.json` exits 0.
Execution coverage adds cache/schema tests and exact plan assertions. Commands,
hashes, failures and profiles are preserved in the new audit receipts.

## Cached invariant reads

The same bounded 32-entry cache now includes per-event invariant SELECTs. SQL
execution distinguishes scalar rows from write completion and clears bindings
after each call. `scalar_steps` identifies reads included in profile counters;
older write-only counters remain in their original receipts.

The measured candidate that cached both invariant and recursive_triggers reads
reduced row-event medians from 89.030 to 58.658 ms, while source-view batch medians
were 26.471 and 27.682 ms. The retained path caches invariants and keeps PRAGMA
direct. Three alternating pairs measured row-event totals 83.055/53.337 ms and
source-view batch totals 24.430/25.221 ms. No batch improvement is claimed.
Both candidate receipts and all exact oracle checks are preserved.

A configuration-toggle regression proves that disabling recursive_triggers after
warming the cache rejects source writes. Four cache tests, the other 99 test
methods, the plan audit and shared semantic/circuit checks pass. Gate receipt
`/tmp/sprefa-sqlite-competitive/gate-cesna0ck/receipt.json`, exit 0.

Final seven-arm Bash batch1000 run, two repetitions, constrained budget, medians:
DD 1.170 ms (volatile), PG query 24.052, pg_ivm 21.329, Take 1 31.156, Take 2
206.508, shadow batch 71.902, source views 35.395 ms. Every admitted state matches
the same input/output hashes. This run is retained separately from prior paired
runs; no cross-run timing normalization is applied. `invariant-shared.jsonl`
contains the exact command, extension hashes, durability and memory/disk scope.
No external blocker occurred. The remaining contracts listed above stay explicit.

## Resumed acceptance work

Parent requested continuation from `68401f9f4`. No pending frontier diff existed
in this worktree. Added partial input sealing across nested savepoint release and
rollback: side a stays sealed while b/c restore their earlier frontiers, b accepts
a new write, and the final committed join equals SQL. Focused source-view frontier
suite: 18 tests pass, exit 0. Command:
`TAKE2_SOURCE_VIEWS=1 TAKE2_EXTENSION=/tmp/sprefa-sqlite-competitive/invariant-cache.dylib python3 v6/labs/exec_shootout/postgres_pglite_ivm/43_sqlite_competitive/3b_frontier_test.py`.
Receipt: `receipts/frontier-resume.log`. Remaining acceptance work continues.

## Executable public-ABI lazy flush boundary

Frontier regression commit: `e5827b9f5`. Added a separate take2_lazy module with
automatic delta queueing, read-time flush and xSync preparation flush. No public
hook is replaced. Ordinary source forwarding triggers and SQLite shadow storage
remain. Tests cover precommit reads after completed statements, autocommit
conflicts including partial FAIL, failed reads after maintenance, source-trigger
prefix reads followed by ABORT, savepoints, second writers and missing module.

Gate `/tmp/sprefa-sqlite-competitive/gate-hh86gdqt/receipt.json` exits 0:
135 test methods plus plan audit; 171 semantic states per three arms; 143 circuit
states per four arms. The new lazy tests add 15 methods per storage layout.
The first standalone runner call omitted required IVM_RUN_ROOT and exited 1;
the corrected call and full gate pass, with the failure retained.

Shared Bash batch1000, three repetitions: PG query 22.410 ms, pg_ivm 20.000,
volatile DD 0.975, explicit source views 29.285, lazy source views 30.373.
Every state matches exact shared hashes. `receipts/lazy-paired.jsonl` records the
complete command, hashes and telemetry; exit 0. The public read/preparation path
is now executable evidence in the finite ledger. It does not close the measured
SQLite/pg_ivm gap. A task-local debug SQLite lifecycle probe follows.

## Pinned SQLite debug build probe

`python3 v6/labs/exec_shootout/postgres_pglite_ivm/43_sqlite_competitive/2b_sqlite_build.py`
builds outside the source tree with --debug --fts5 --session and the existing Tcl
installation. No source patch or installation occurs. Git source SHA remains
f3b9f74d81132426dee1ccc07a67fdad2ccfeaa9; SQLite reports fossil source ID
4021369bc9558fbfcfa83ee4cd6b986734b8c41b6d72bd97064e566ca8189ea8.
The generated library and build commands are hashed in the receipt.

Upstream savepoint/savepoint2/conflict/conflict2/FTS5 savepoint/FTS5 conflict suites
pass 140+182+148+140+3+27 = 640 tests with zero errors and no memory leaks reported.
Python loads the task-local library and asserts SQLITE_DEBUG is enabled.

`DYLD_LIBRARY_PATH=/tmp/sprefa-sqlite-competitive/sqlite-debug-3dwv00ri python3 v6/labs/exec_shootout/postgres_pglite_ivm/43_sqlite_competitive/5_gate.py`
passes: `/tmp/sprefa-sqlite-competitive/gate-p748v51g/receipt.json`, exit 0.
This runs the full 135-method competitive gate and shared oracles under the
debug SQLite runtime, including lazy xFilter/xSync writes. The stock arm remains.
Source/library hashes and upstream logs are preserved in debug-build receipts.
This closes the custom-callback necessity question for the tested read/commit
contract. It does not claim a general statement-end notification API.

## General SELECT lowering and source-view initialization repair

Reuses Take 1's sqlite3-parser 0.17.0 (Unlicense) AST and expression serializer;
serde_json 1.0.149 serializes a bounded version-1 plan. Cargo.lock pins transitive
dependencies. Compiler/build directories are task-owned. The compiler emits
configuration once and performs no database maintenance. Source schema binding,
delta masks, scheduling and persistent storage remain separate implementation
units. Supported fragment: two scalar projections, one to three main table
occurrences, INNER/CROSS/theta joins, repeated sources, WHERE and admitted scalar
expressions. Aggregates, negation, recursion and epochs retain their dedicated
mode APIs; general SELECT lowering for these shapes remains unsupported.

Initial oracle run failed eight subcases before generic delta execution. After
that implementation, first-ever multi-row source-view insertion exposed an index
initialization defect in the earlier lazy path: CREATE INDEX ran inside a source
row trigger, omitted entries, and later DELETE reported malformed images.
`compiler-sourceview-failure.log` preserves this exit-1 result. No user DB was
opened. The repaired lazy layout requires direct-only `take2_prepare(name)`
before source DML. Omitted setup and trigger invocation fail; preparation rollback
requires preparation again. First multi-row insert/reopen/delete passes exact
oracle and PRAGMA integrity_check. Prior lazy benchmark receipts predate this fix.

Current gate: `/tmp/sprefa-sqlite-competitive/gate-v969vz3q/receipt.json`, exit 0.
147 Python methods and 2 Rust tests pass, plus existing shared circuits/oracles.
Compiler oracle covers six actual SELECTs, including inequality, expressions,
self/multiway cross terms, NULLs, savepoint rollback, reopen, and retraction.
Focused source-view compiler tests also pass under the pinned SQLITE_DEBUG build.
CI execution coverage adds compiler build/tests and two layout regressions.

Commands:
`CARGO_HOME=/tmp/sprefa-sqlite-competitive/cargo CARGO_TARGET_DIR=/tmp/sprefa-sqlite-competitive/compiler-target cargo test --locked --manifest-path v6/labs/exec_shootout/postgres_pglite_ivm/43_sqlite_competitive/6_sql_compile/Cargo.toml`
and the reproducible `5_gate.py` command above. Retained red/green logs and gate
hashes are in `43_sqlite_competitive/receipts/compiler-*`.
