# SQLite IVM plugin handoff, 2026-09-08

Implemented bounded SQL SELECT installation, persistent arithmetic join COUNT/SUM
maintenance, semantic rejection inventory, connection-scoped telemetry and the
shared five-arm shootout. General SQL coverage remains restricted to the admitted
slice below. No compiler/kernel semantics changed. No additional workers, merge,
push, global installation, user database edits or global settings changes.

Implementation commits:

- `b1bba8133`: loadable SQL installer, AST/catalog binding, signed triggers,
  lifecycle/metrics telemetry and initial acceptance tests.
- `3f858cf50`: shared plugin arms, expanded semantics, 35-family matrix,
  foreign-key rejection, generalized reports and the complete gate.

- `3c6d6781a`: assigned-key bounds regression, final binary semantic/grid
  receipts, stock SQL consumer and provenance.

This handoff additionally validates assigned INTEGER PRIMARY KEY values after
BEFORE INSERT, with a stored-image bounds regression. It adds final-binary receipts, provenance and a stock SQL consumer
example. Parent owns sampled review and integration.

## Build and SQL consumer

Worktree: `/Users/chrishafley/projects/sprefa/.boop-worktrees/feature/sqlite-ivm-astra`.

```sh
export CARGO_TARGET_DIR=/private/tmp/sqlite-ivm-astra-target
cargo build --release --locked \
  --manifest-path v6/labs/exec_shootout/postgres_pglite_ivm/25_sqlite_ivm/Cargo.toml
/opt/homebrew/opt/sqlite/bin/sqlite3 :memory:
```

```sql
.load /private/tmp/sqlite-ivm-astra-target/release/libsqlite_ivm.dylib
PRAGMA recursive_triggers=ON;
PRAGMA trusted_schema=ON;
CREATE TABLE fact(id INTEGER PRIMARY KEY,group_id INTEGER,amount INTEGER);
CREATE TABLE dimension(group_id INTEGER PRIMARY KEY,factor INTEGER);
INSERT INTO fact VALUES(1,1,4),(2,1,-1);
INSERT INTO dimension VALUES(1,2);
SELECT sqlite_ivm_create('totals',
 'SELECT f.group_id,COUNT(*) AS row_count,SUM(f.amount*d.factor) AS weighted_sum
  FROM fact f JOIN dimension d ON f.group_id=d.group_id GROUP BY f.group_id');
SELECT * FROM totals;                 -- 1|2|6
UPDATE dimension SET factor=-3;
SELECT * FROM totals;                 -- 1|2|-9
SELECT sqlite_ivm_drop('totals');
```

Actual release extension: `/private/tmp/sqlite-ivm-astra-target/release/libsqlite_ivm.dylib`, 1308768 bytes,
SHA256 `e936a041995e293a88a2bc6793e80539078cf1b3d989d1287f17434a9dafb8d0`. Stock CLI SQLite 3.53.2, extension minimum 3.37;
ABI from rusqlite 0.40.2 / libsqlite3-sys 0.38.2, parser sqlite3-parser 0.17.0.
Compiler: `rustc 1.100.0-nightly (17fd5b8a3 2026-08-28)`. Exact SQLite source ID, lock/source/binary hashes,
linker dependencies and tool versions are in
[release-provenance.json](/Users/chrishafley/projects/sprefa/.boop-worktrees/feature/sqlite-ivm-astra/v6/labs/exec_shootout/postgres_pglite_ivm/results/plugin-20260908/release-provenance.json). The macOS link metadata names
system libsqlite3; `nm -u` reports zero unresolved SQLite function symbols, and
`sqlite_ivm_version()` reports the host CLI's 3.53.2 API. Apple /usr/bin/sqlite3
has `.load` disabled; the existing Homebrew stock executable was used.

The retained CLI example creates `/Users/chrishafley/projects/sprefa/.boop-worktrees/feature/sqlite-ivm-astra/v6/labs/exec_shootout/postgres_pglite_ivm/results/plugin-20260908/release-consumer.sqlite`.
A second stock CLI process, without `.load`, sets the writer pragmas and updates
fact: result `1|2|-27`, fresh recomputation equality `1`. SQL/stdout/stderr receipts
are `release-consumer-install.*` and `release-consumer-second-writer.*` in the same directory.
No Prolog process, host flush or host maintenance loop is required for writes.

Public signatures (SQL scalar functions, DIRECTONLY):

| Signature | Result / ownership |
|---|---|
| sqlite_ivm_create(name TEXT, select_sql TEXT) | name TEXT; atomic schema/population installation |
| sqlite_ivm_drop(name TEXT) | name TEXT; atomic removal of owned objects |
| sqlite_ivm_version() | JSON TEXT with build/API/algorithm/writer contract |
| sqlite_ivm_log(event_limit INTEGER) | accepted limit 0..10000; connection-local allowance reset |
| sqlite_ivm_metrics(name TEXT, enabled INTEGER) | accepted 0/1; per-view transactional setting |

Names bind to real catalog tables/columns; aliases and quoted identifiers are
exercised. Input SQL is parsed as exactly one SELECT and never concatenated into
schema statement text. Generated identifiers are quoted. Installation/drop use
savepoints and preserve caller transactions, including failure cleanup.

Public result departure: a read-only SQL VIEW exposes the maintained ordinary
accumulator table. Direct result writes reject. The earlier proposal named an
ordinary writable result table; the view prevents accidental user edits while
retaining ordinary SELECT consumption. Reserved `__ivm_` internal tables remain
administrative state, outside the direct-DML contract.

## Admitted semantics and maintenance sequence

Two different main-schema ordinary tables, single integer equality ON or USING,
one GROUP BY join key, projection key / COUNT(*) / SUM(column or two-column
product). Aggregate aliases required. All source columns declare INTEGER; stored
values after affinity are non-NULL integers within ±1,000,000. Each intermediate
maintained group has at most 1,000,000 supports; SUM fits ±10^18. Text integer
inputs converted by INTEGER affinity are admitted; fractional/text/blob/NULL and
out-of-bound stored values reject. Duplicate supports on both sides are retained.
Rowid, STRICT, WITHOUT ROWID and their combination are tested.

1. Bind AST to main catalog; reject unsupported shape/type/collation/generated
   columns, foreign keys, unmanaged source triggers and TEMP source shadows.
2. Validate initial values and groups; check schema collisions. Create accumulator,
   scratch delta table, fixed metadata/counters row, read-only view, source indexes
   and guards/maintenance triggers. Initial population evaluates the full join.
3. Ordinary source BEFORE triggers enforce recursive_triggers and row bounds.
   trusted_schema OFF makes SQLite reject the pragma-virtual-table guard.
4. AFTER triggers revalidate assigned row images; INSERT joins NEW against the opposite source; AFTER DELETE joins OLD.
   UPDATE subtracts OLD contributions, then adds NEW, including key moves.
5. SQL adds signed counts/sums to accumulator rows; missing positive groups are
   inserted; invalid bounds abort; zero-support groups are deleted. Scratch rows
   are cleared before returning. No full affected-group refresh occurs here.
6. Optional SQL counters share the same transaction. Rollback restores inputs,
   accumulators and counters. Reopen/second writers use persistent triggers.

The existing compiler's signed accumulator SQL pattern was inspected and reused
conceptually; its single-positive-source AVG delta template does not bind this
join. New lowering lives solely in the lab extension. The old compiler-generated
SQLite affected-group arm and all prior receipts remain preserved.

[28_semantic_coverage.md](/Users/chrishafley/projects/sprefa/.boop-worktrees/feature/sqlite-ivm-astra/v6/labs/exec_shootout/postgres_pglite_ivm/28_semantic_coverage.md) inventories 35 families
with concrete tests and source links. DD library expressibility, inferred operator
composition, executed engine coverage and rejected SQL are separate statuses.
The DD benchmark advances one u64 frontier per state; arbitrary DD times and
iteration are not certified by that fixture.

Rejected/missing families include standalone projection/filter, general expressions,
DISTINCT/count-distinct, self/multiway/outer joins, semijoin/antijoin, set operations,
AVG/MIN/MAX, global aggregates, NULL algebra, subqueries/CTEs, recursion/cyclic
deletion, ordering/top-k/windows, and DD timestamp APIs. P08 contains 32 named
query rejections; numeric, catalog and setting rejections have separate tests.
The matrix records the required missing lowering algorithms. No kernel decision
was needed for the delivered slice.

## Current validation and coverage

[Complete gate](/Users/chrishafley/projects/sprefa/.boop-worktrees/feature/sqlite-ivm-astra/v6/labs/exec_shootout/postgres_pglite_ivm/results/plugin-20260908/final-gate-3/status.txt): **55/55 passing**, zero skipped.
Release extension and actual DD example compile successfully.

| Executed suite | Tests |
|---|---:|
| Loaded extension 27_sqlite_ivm.test.py | 24 |
| Existing compiler-SQL transport 20_sqlite_template.test.py | 6 |
| Stock SQLite mechanisms 17_sqlite_trigger_capabilities.py | 10 |
| Shared fixture/DD/plugin/report integration 23_crossover.test.mjs | 5 |
| Existing sprefa-store datalog_ops SQL reference | 6 |
| Existing sprefa-store oracle_dd reference | 4 |

Coverage adds the loaded extension and shared plugin arm tests, changes the shared
semantic fixture, and reuses existing reference tests. No CI workflow was changed
to invoke them automatically. Local reproduction:

```sh
bash v6/labs/exec_shootout/postgres_pglite_ivm/29_plugin_gate.sh /private/tmp/fresh-ivm-gate
```

Plugin tests cover 240 seeded two-sided bag transitions, both row images, duplicate
supports, zero/negative SUM, retract-to-zero, conflict policies
REPLACE/UPSERT/IGNORE/FAIL/ABORT/ROLLBACK, savepoints, failed installation/drop,
multiple views, collisions, independent connections, required settings, numeric
caps (including assigned primary keys), affinity/collation rejection, and storage variants. Foreign-key cascades
explicitly reject because their nested writes can interleave join inputs before
AFTER maintenance. Telemetry tests include success, rejection, rollback, a second
writer, disabled mode, closed stderr and retained host trace ownership.

Injected detectors: removing a DD keyed write and a plugin SQL write causes
nonzero exits; direct test-only accumulator corruption fails recomputation.
P02 preserves seeded failing traces under SQLITE_IVM_FAILURE_ROOT. The observed
JavaScript negative-zero test failure is retained in m2-integration-tests.log;
the fix normalizes integer zero. Post-fix serialized fixture equality is recorded
in m2-fixture-normalization.log; the final binary semantic run also passes.

Final shared semantic receipt: **171 states per arm, 855 exact input/output
validations**, five arms. This adds shared duplicate support/retraction, multirow
dimension negation, dimension-key move, empty inputs and reseed states. Exact
checks compare actual inputs, output state and canonical hashes to full
recomputation after each admitted transition.

## Shared performance receipts

[Chart](/Users/chrishafley/projects/sprefa/.boop-worktrees/feature/sqlite-ivm-astra/v6/labs/exec_shootout/postgres_pglite_ivm/results/plugin-20260908/release-all-arms.svg) · [all-arm table](/Users/chrishafley/projects/sprefa/.boop-worktrees/feature/sqlite-ivm-astra/v6/labs/exec_shootout/postgres_pglite_ivm/results/plugin-20260908/release-all-arms.tsv) ·
[logging overhead](/Users/chrishafley/projects/sprefa/.boop-worktrees/feature/sqlite-ivm-astra/v6/labs/exec_shootout/postgres_pglite_ivm/results/plugin-20260908/release-logging-overhead.tsv) ·
[raw trials](/Users/chrishafley/projects/sprefa/.boop-worktrees/feature/sqlite-ivm-astra/v6/labs/exec_shootout/postgres_pglite_ivm/results/plugin-20260908/release-repeated.jsonl) · [semantic run](/Users/chrishafley/projects/sprefa/.boop-worktrees/feature/sqlite-ivm-astra/v6/labs/exec_shootout/postgres_pglite_ivm/results/plugin-20260908/release-semantic.jsonl).

Final grid: nine cells ≤12,000 rows, five measured repetitions per arm plus one
warmup. **45 matched five-arm trials**, 225 measured cases, 45 warmup cases,
1,350 exact state checks including warmups. Zero engine errors, mismatches,
timeouts or resource-blocked cases. Arms rotate through all five positions over
five measured repetitions. Previous reports/receipts remain untouched.

Median milliseconds for four mutation transactions plus result materialization
and count; setup, oracle/hash validation and initial query excluded:

| Rows | Batch | Fanout | pg_ivm | SQLite affected-group | DD volatile | Plugin disabled | Plugin logged |
|---:|---:|---:|---:|---:|---:|---:|---:|
| 400 | 10 | 10 | 5.890 | 1.744 | 0.140 | 1.513 | 1.358 |
| 12000 | 10 | 10 | 11.304 | 4.236 | 0.315 | 2.291 | 2.159 |
| 400 | 10 | 200 | 7.159 | 1.770 | 0.148 | 1.553 | 1.660 |
| 1200 | 10 | 200 | 7.362 | 2.669 | 0.156 | 1.711 | 1.545 |
| 4000 | 10 | 200 | 6.914 | 3.567 | 0.170 | 1.812 | 1.813 |
| 12000 | 10 | 200 | 7.905 | 4.008 | 0.251 | 1.836 | 1.980 |
| 12000 | 1000 | 200 | 27.802 | 174.194 | 1.151 | 28.517 | 28.557 |
| 12000 | 1 | 200 | 7.807 | 2.297 | 0.228 | 1.627 | 1.709 |
| 12000 | 100 | 200 | 10.808 | 20.042 | 0.399 | 5.998 | 6.417 |

PG and SQLite use durable commits. DD is volatile and has no durable commit or
reopen guarantee. SQL timers include ordinary transaction submission; native DD
uses keyed in-process writes, arranged join, signed tuple CountTotal and probe
completion. SQLite plugin and affected-group arms share the same Python SQL
transport and oracle. Hosts do not compute maintenance. Initial plugin install
and extension loading are included in setup, outside mutation timings.

Final semantic runner wall: 1.755 seconds. Final grid runner wall: 39.238 seconds.
Each process has a 120-second cap; each sweep a 20-minute deadline. The existing
memory-pressure guard observed 51–58% free memory during the final grid and stops
remaining rows below 15%. No larger sweep was launched. Total RAM is unenforced;
SQLite caches and PG tuning settings are not total-memory caps.

| Arm | Process/observed RSS scope | Closed DB bytes range |
|---|---|---:|
| pg_ivm | sampled postmaster tree peak 69,616 KiB; client measured separately | 7,968,447..11,794,111 |
| SQLite affected-group | Python+SQLite+fixture+oracle process peak 49,922,048 B | 139,264..598,016 |
| Plugin logging disabled | same process scope, peak 49,430,528 B | 61,440..335,872 |
| Plugin logged | same process scope, peak 50,659,328 B | 61,440..335,872 |
| DD | process includes graph/maps/fixture/oracle, peak 27,607,040 B; periodic samples unavailable | 0 volatile |

SQLite WAL/synchronous FULL and PG fsync/synchronous_commit/full_page_writes are
recorded. SQLite main/WAL/SHM sizes are sampled before close plus closed DB size;
SQLite temp-file bytes remain null, explicitly unsampled. PG database/WAL/temp
receipts use the existing server queries. Setup times, fanout, batch, hot-group
skew description, churn states and output cardinality are retained per case.
Full source/extension/lock hashes are in release-provenance.json. Runtime databases
and clusters were task-owned. Existing task-local PG binaries were reused via
IVM_POSTGRES_PREFIX; fresh /tmp/pgx.* clusters were stopped and removed by the
shared Bash cleanup, with logs retained.

## Logging controls and measured overhead

SQLITE_IVM_LOG_LIMIT defaults to 0, capped at 10000 events per loaded connection.
sqlite_ivm_log resets the allowance. Scoped tracing JSON goes to stderr at
load/install/bind/lower/schema/savepoint-release/installation-rollback/drop
boundaries with operation/view identifiers, extended code and elapsed duration.
The library installs no process-global subscriber and replaces no host SQLite
trace/commit/rollback callback. Errors writing diagnostics do not fail SQL.
Operation IDs restart per loaded connection; no globally unique connection ID
is supplied. No row values or expanded SQL are emitted in diagnostic streams; payload opt-in
is unimplemented. Benchmark stdout retains its requested SQL/JSONL validation
protocol and exact result snapshots separately from diagnostics.

sqlite_ivm_metrics enables a fixed per-view SQL row counting operations, absolute
join contributions and touched groups (OLD/NEW counted separately). Counters
saturate at 10^15; rollback restores them and ordinary second writers update them.
They do not grow persistent per-row logs. SQL queries can read the metadata row
without an extension. The plugin cannot observe arbitrary writer commit/rollback
callbacks or durations; the benchmark reports boundaries of transactions it
itself submits. Install-release events identify savepoint release, not outer
commit. Public output insertion/retraction counts are measured from successive
verified SQLite result snapshots outside timing.

The logged arm uses event limit 32 plus transactional counters. Mutation ratios
in release-logging-overhead.tsv include the counters' SQL work; process-wall ratios
also include lifecycle/stage stderr and setup. At 12k/batch1000/fanout200 the median
paired logged/disabled mutation ratio is **1.056**, range **0.900..1.097**; at
batch100 it is **1.070**, range **0.973..1.224**. Other cells include ratios below
one and noisy ranges; these samples do not isolate a constant logging cost.
The raw disabled/logged trials are separately labelled and never pooled.

## Filesystem reading order and remaining limits

All relative files below are beneath `/Users/chrishafley/projects/sprefa/.boop-worktrees/feature/sqlite-ivm-astra/v6/labs/exec_shootout/postgres_pglite_ivm`:

| Reading order | File / role |
|---|---|
| 9, 12, 13 | shared fixture, runner and sole Bash shootout entry |
| 14, 15 | existing summary/chart commands, now including all-arm and logging outputs |
| 18, 19, 20 | preserved compiler fixture/affected-group transport/tests; 19 shares timing/oracle transport |
| 22, 23 | actual native DD example and shared integration tests |
| 25/0_README.md, 25/src/0_types.rs | extension contract, bound types and identifier handling |
| 25/src/1_bind.rs, 2_lower.rs | parser/catalog admission, SQL trigger generation |
| 25/src/3_telemetry.rs, lib.rs | scoped tracing, public ABI/lifecycle functions |
| 25/1_example.sql, 26_sqlite_plugin_adapter.py | SQL consumer example and loaded-plugin benchmark installation |
| 27_sqlite_ivm.test.py, 28_semantic_coverage.md, 29_plugin_gate.sh | acceptance, source-backed matrix, complete gate |

Absolute receipt root: `/Users/chrishafley/projects/sprefa/.boop-worktrees/feature/sqlite-ivm-astra/v6/labs/exec_shootout/postgres_pglite_ivm/results/plugin-20260908`. Each release-repeated.artifacts/cases-constrained/
case directory retains fixture.json, stdout/stderr, and for SQLite its installed
schema and maintained.sqlite. SQL state survives process exit; DD graph state
ends with its process. WAL/SHM are adjacent while databases are live; test-owned
TemporaryDirectory databases are removed by test cleanup. No runtime binary is
installed globally; rebuild the extension from its independent crate if the
lane target is removed.

Remaining limits are explicit: general lowering families in the matrix; reserved
internal schema ownership and migrations; administrative DDL/function overrides
or C APIs bypassing SQL triggers; crash/power-loss testing; arbitrary DD times;
SQLite temp-byte observability; expanded-SQL/row diagnostic opt-in; and upstream
parser license metadata discrepancy (manifest Apache-2.0/MIT, packaged Unlicense).
The admitted slice and enumerated rejection/telemetry/shared-shootout work have
executable receipts. Broader plugin semantics remain unimplemented.

## Continued circuit SQL milestone (2026-09-08)

The `circuits` profile in the existing `13_crossover_run.sh` now runs 11
families from `30_circuit_workload.mjs`, each with 13 exact transitions.
The independent JavaScript oracle preserves output bags. SQLite full-query,
PostgreSQL full-query, pg_ivm and the actual loaded plugin receive identical
SQL writes; each validates actual source rows and projected output rows.
`31_circuit_sqlite.py`, `32_circuit_postgres.mjs`, then
`33_circuit.test.mjs` are the new adapter/test reading order.

| Family | SQLite full query | PG full query | pg_ivm | SQLite plugin |
|---|---|---|---|---|
| pipeline/filter/project | executed | executed | executed | rejected |
| fanout/fanin UNION ALL | executed | executed | rejected | rejected |
| projected DISTINCT | executed | executed | executed | rejected |
| equijoin bag | executed | executed | executed | rejected |
| self join | executed | executed | executed | rejected |
| three-table chain | executed | executed | executed | rejected |
| diamond UNION ALL | executed | executed | rejected | rejected |
| semijoin EXISTS | executed | executed | executed | rejected |
| antijoin NOT EXISTS | executed | executed | rejected | rejected |
| grouped aggregate churn | executed | executed | executed | executed |
| recursive reach/cycle/root deletion | executed | executed | rejected | rejected |

Receipts: absolute artifact root
`/Users/chrishafley/projects/sprefa/.boop-worktrees/feature/sqlite-ivm-astra/v6/labs/exec_shootout/postgres_pglite_ivm/results/plugin-circuits-20260908/`.
`sql-gate.jsonl` has 390 exact engine-state checks, 14 explicit installation
rejections, and 11 successful admitted-arm comparisons. Unsupported cases do
not count as parity. `sql-gate.tsv` and `sql-gate.svg` are generated by existing
14/15 commands and preserve family identity. `initial.jsonl` retains a caught
adapter error: pg_ivm internal support columns were accidentally selected;
the adapter now selects only declared c0/c1/c2 outputs.

`33_circuit.test.mjs` adds four tests: all 143 SQLite transitions, literal bag
fanin/DISTINCT expectations, cyclic root retraction/restore, and an injected
incorrect expected bag that the adapter must detect. `29_plugin_gate.sh`
includes these tests. The circuit SQL baseline recomputes the full query;
it claims no incremental algorithm. Sizes remain 24 source-a rows with
7 rows each in b/c initially, batch 3 and fanout 4. Timings are correctness
smoke receipts, with no performance conclusion from one repetition.

DD and SWI circuit adapters, broader aggregate/window/NULL contracts,
frontier-specific execution, circuit scale sweeps, and circuit telemetry
expansion remain pending. Existing DD/plugin grouped-join receipts and logging
overhead remain preserved. No compiler/kernel changes were made.

Current complete gate: `plugin-circuits-20260908/full-gate/status.txt` PASS,
59 tests (55 preserved plus 4 circuit tests), builds unchanged Rust plugin
and DD example. Final focused SQL runner gate `sql-final.jsonl` passes after
restricting expected plugin rejection messages; 390 exact states and 14
rejections. This milestone adds lab gate coverage, with no CI workflow edits.

## Native DD circuit milestone

SQL milestone commit: `a1f0d07d3`. Native DD circuit source is
`34_circuit_dd.rs`, registered as the `circuit_dd` lab example in the existing
store manifest. All 11 families now execute actual DD operators against the
same fixture. Semijoin/antijoin use distinct right keys for presence semantics;
reachability reuses `src/oracle.rs`'s `roots.iterate` / semijoin / distinct
pattern. No oracle rows enter the operator graph. Observed input collections
and consolidated output multiplicities are compared exactly after every epoch.

`dd-final.jsonl`: 533 exact engine-state checks (390 SQL plus 143 DD), 14
explicit unsupported SQL installation cases, 11 matched admitted-family runs,
and 11 finite scalar frontier checks. `dd-full-gate/status.txt` PASS, 60 tests
(59 preceding plus the DD catalog/frontier test). The DD adapter is volatile,
one worker, sequential u64 epochs. A startup check holds c's input frontier
while advancing a/b, asserts the combined probe remains incomplete, then
releases c and awaits completion. This covers one scalar frontier boundary;
partial orders, independent time dimensions and arbitrary DD operators remain
outside this executable receipt. The fixture's recursive cycle deletion is
executed inside DD's iteration scope.

Use `--arms query,pg_ivm,sqlite-query,sqlite-plugin-delta,dd` with profile
`circuits`, existing `--sqlite-extension`, and
`--circuit-dd-bin /private/tmp/sqlite-ivm-astra-target/release/examples/circuit_dd`.
The new DD binary is separate from preserved `crossover_dd`. Current circuit
sizes remain correctness smoke sizes. SWI and scale sweeps remain pending.
