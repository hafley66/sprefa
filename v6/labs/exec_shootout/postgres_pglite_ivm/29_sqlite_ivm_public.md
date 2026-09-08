# Declared-plan SQLite IVM public surface

2026-09-08. Tested executable commit `094e46304`; final documentation and test
receipt commit follows this report. The worktree is
`/Users/chrishafley/projects/sprefa/.boop-worktrees/feature/postgres-ivm-crossover`.

## Result

`sqlite_ivm.install(connection, specification)` now installs a versioned,
declared relational plan into a caller-provided SQLite database. Installation
creates or validates source tables, compiles the admitted algebra to SQL,
populates hidden result storage, exposes a read-only result view, creates
persistent source triggers, and records bounded metadata and maintenance
counters. Ordinary writes after installation require only stock SQLite. No
daemon, Prolog compiler, Python update loop, connection callback, loadable
extension, or writer pragma is required.

The maintenance algorithm is `full-query-recomputation-per-source-row`.
Every source INSERT, DELETE, or UPDATE deletes and repopulates that result's
hidden storage inside the source statement transaction. This is not arithmetic
delta maintenance and is not reported as such. The older compiler-emitted
affected-group arm remains unchanged as `sqlite-template-group`.

The current task authorized a relational plan/schema installer. The earlier
loadable-extension proposal in `TASKS/3_sqlite-ivm-plugin.BRIEF.md` remains
unimplemented: there is no `sqlite_ivm_create(name, select_sql)` scalar function
and no parser for arbitrary SQL text.

## Public API and lifecycle

```python
from sqlite_ivm import compile_plan, drop, install

compiled = compile_plan(specification)       # -> CompiledPlan
installed = install(db, specification, trace=None)  # -> InstallResult
drop(db, "summary", trace=None)              # -> None
```

`db` is an existing `sqlite3.Connection`. Both install and drop use savepoints,
preserve an outer transaction, and remove partial objects on error. A
`PlanError` contains a stable `code` and `detail`. Multiple installed results
may depend on the same source. Persistent triggers work after reopen and from
an independently opened connection. Created source and result-storage tables
use SQLite STRICT mode. Existing sources are catalog-checked and scanned for
non-integer values before installation. BEFORE triggers reject later non-null
integer contract violations. Aggregate overflow or a REAL result fails the
STRICT result write and rolls back the source statement.

The public result is a SQLite view. Direct INSERT, UPDATE, or DELETE against it
fails through SQLite's ordinary non-updatable-view behavior. Drop removes its
triggers, view, hidden storage, dependency rows, and runtime row.

## Accepted relational plan shapes

Version 1 plans admit the following finite algebra, nested through each node's
`input`, `left`, and `right` fields:

| Class | Accepted form |
|---|---|
| Schema | Main-schema ordinary tables; explicit columns; non-null signed SQLite INTEGER storage; optional declared primary key; create-if-missing opt-in |
| Scan | `{"op":"scan","table":"fact","as":"f"}`; aliases namespace outputs as `f__column` |
| Filter | Boolean predicate over the input's named columns |
| Projection | Explicit unique output names; physical duplicate rows are preserved |
| Join | Inner joins; nested self joins and multiway joins; explicit predicate |
| Aggregate | Grouped or global COUNT(*) and SUM(integer expression); zero SUM values retained; grouped last-contributor deletion removes the group; global empty input returns COUNT 0 and SUM NULL |
| Expressions | Column, signed 64-bit integer literal, add, subtract, multiply, modulo, equality/inequality/order comparison, AND, OR, NOT |
| Transitions | INSERT, DELETE, UPDATE, key/group moves, multirow statements, explicit transactions/savepoints, rollback, REPLACE, UPSERT, IGNORE, and constraint ABORT |
| Multiplicity | Projection preserves bag rows physically; COUNT and SUM consume every joined row, including equal projected values from different keyed inputs |

Relational nodes lower to explicit derived-table subqueries, so filter,
projection, aggregate, self-join, and multiway-join nesting is supported within
the admitted operators. The installer accepts this declared plan rather than
SQL text, which prevents an unparsed SQL fragment from crossing a statement
boundary.

## Rejected shapes and diagnostics

| Diagnostic | Rejected class |
|---|---|
| `IVM000_VERSION` | Unknown plan version |
| `IVM001_IDENTIFIER` | Empty/NUL identifier or reserved result prefix |
| `IVM003_FIELD` | Unknown field at any plan node |
| `IVM010_COLUMN` | Unbound or unavailable column |
| `IVM011_LITERAL`, `IVM012_RANGE` | Non-integer literal or integer outside signed 64-bit range |
| `IVM013_EXPRESSION` | Division, functions, nondeterminism, casts, collations, CASE, scalar subquery, EXISTS expression |
| `IVM021_JOIN_COLUMN` | Join inputs with overlapping output names; distinct scan aliases resolve self joins |
| `IVM023_TYPE`, `IVM024_NULL` | Text/real/blob sources or nullable sources |
| `IVM030_AGGREGATE` | AVG, MIN, MAX, COUNT(expr), DISTINCT aggregate, user aggregate |
| `IVM031_OPERATOR` | Outer/semi/anti join, negation, UNION/set operations, DISTINCT, ORDER/LIMIT/top-k, window, CTE, recursive/fixpoint, scalar/correlated subquery |
| `IVM040`–`IVM047` | Catalog mismatch, existing invalid value, object collision, missing/wrong-kind source, missing installed result |

Foreign keys, floating arithmetic, text/collation semantics, nullable joins and
aggregates, cyclic dataflow, DD time/frontier APIs, concurrent competing writer
stress, schema migration, trigger tampering, `writable_schema`, and crash or
power-loss injection remain outside the accepted contract.

## Shared semantic result

The `semantic` profile ran the same 163-state fixture through pg_ivm,
`sqlite-plan-refresh`, and native DD. It performed 489 exact input/output state
checks. All 489 passed; exact mismatches, engine errors, timeouts, and resource
blocks were zero. The sequence covers the five crossover phases, delete and
reinsert, primary-key/group move, zero SUM with positive COUNT, last-group
removal, dimension delete/reinsert, and 150 seeded changes across seeds 7, 42,
and 2026.

The five public API tests additionally execute filtered aggregation, physical
bag projection, self join, three-way join, global-empty aggregation, multiple
results, direct-result-write rejection, independent connection writes,
transaction rollback, REPLACE/UPSERT/IGNORE/ABORT, type failure, overflow
rollback, install cleanup, drop, and 16 rejected-class cases. Those additional
plan shapes are SQLite API tests; no pg_ivm/DD parity claim is made for them.

An injected receipt changed one expected input hash to 64 zeroes. The adapter
exited 1 at the first changed state and printed both the observed and injected
hash. Red and green raw evidence is retained under
`/private/tmp/sqlite-ivm-finish.vwv3qa/`.

## Repeated performance

Each cell used one discarded warmup and five measured repetitions per arm,
rotated arm order, the same five states, exact fixture/oracle, size, fanout,
batch size, and timer boundary. There were 45 successful triples and 675
measured state checks with zero mismatches. Medians are milliseconds.

| Rows | Batch | Fanout | pg_ivm durable SQL | SQLite plan full-refresh | SQLite commit portion | DD volatile |
|---:|---:|---:|---:|---:|---:|---:|
| 400 | 10 | 10 | 5.636 | 3.540 | 0.504 | 0.134 |
| 12,000 | 10 | 10 | 8.687 | 84.192 | 0.488 | 0.258 |
| 400 | 10 | 200 | 10.680 | 3.168 | 0.712 | 0.148 |
| 1,200 | 10 | 200 | 6.736 | 7.769 | 0.232 | 0.154 |
| 4,000 | 10 | 200 | 7.414 | 26.454 | 0.360 | 0.157 |
| 12,000 | 10 | 200 | 16.995 | 83.278 | 0.450 | 0.183 |
| 12,000 | 1,000 | 200 | 23.646 | 8,354.893 | 1.170 | 1.175 |
| 12,000 | 1 | 200 | 8.657 | 11.677 | 0.371 | 0.280 |
| 12,000 | 100 | 200 | 19.199 | 806.096 | 0.856 | 0.372 |

SQL transaction time includes trigger maintenance and durable commit. The
SQLite commit portion is also emitted separately. DD receives in-process keyed
writes, waits for its frontier, materializes maintained output, and has no WAL
or durable commit. Its values remain labeled volatile.

## Memory and storage

| Arm | Observed peak RSS | Process-reported peak | Database bytes |
|---|---:|---:|---:|
| pg_ivm | 64,480 KiB postmaster tree | backend and Node values remain per-state in raw receipt | 8,009,407–11,409,087 plus separately recorded cluster WAL |
| SQLite plan | 47,712 KiB process tree | 49,840,128 bytes | 65,536–339,968 after checkpoint/close |
| Native DD | periodic sample unavailable for short processes | 27,082,752 bytes | 0; volatile |

PostgreSQL settings were `shared_buffers=32MB`, `work_mem=1MB`,
`effective_cache_size=64MB`, `maintenance_work_mem=32MB`, and
`temp_file_limit=2GB`, with fsync, synchronous_commit and full_page_writes on.
SQLite reported `journal_mode=wal`, `synchronous=FULL`, `foreign_keys=ON`,
`temp_store=MEMORY`, `cache_size=-2000`, and `mmap_size=0`. These settings are
tuning and durability controls rather than enforced total-process caps. The
runner recorded total-memory enforcement as unavailable and stopped cases only
if host free memory fell below 15%; no stop occurred.

## Telemetry

The library accepts an optional callback and emits bounded structured records
for bind/lower, install, drop, rejection, SQLite error code, duration, view,
dependencies, trigger count, output rows, and algorithm. It does not install a
global logger or seize a connection trace callback.

Persistent `__sqlite_ivm_runtime` contains one bounded row per installed result:
refresh count, last source, last operation, last change time, and last output
row count. Ordinary writers update that row in the same trigger transaction.
It provides no fabricated BEGIN/COMMIT events. The benchmark adapter records
setup, maintenance/application duration, durable commit duration, query,
independent check, reopen, RSS, and file-size phases. `SQLITE_IVM_TRACE=1`
writes installer records to stderr; stdout remains JSONL protocol. Row values
appear only in benchmark receipts, not library telemetry.

## Reading order and artifacts

1. `sqlite_ivm/0_plan.py`: plan types, diagnostics, validation, SQL lowering.
2. `sqlite_ivm/1_installer.py`: catalog binding, atomic schema/trigger install,
   bounded runtime state, drop.
3. `sqlite_ivm/__init__.py`: public library exports.
4. `25_sqlite_ivm.test.py`: public surface and accepted/rejected semantics.
5. `26_crossover_plan.py`: shared crossover plan declaration.
6. `27_sqlite_ivm_adapter.py`: fixture transport, exact oracle, phase telemetry.
7. `28_public_ivm_summarize.mjs`: deterministic result summarizer.
8. `results/28_sqlite-ivm-public-20260908/`: committed performance, memory,
   semantic, configuration, and artifact hashes.

Raw repeated and semantic receipts remain task-owned under
`/private/tmp/sqlite-ivm-finish.vwv3qa/final-{full,semantic}.jsonl`, with
case databases, installed SQL, fixture, stdout and stderr below their adjacent
`.artifacts/` directories. Committed `SHA256SUMS` records their hashes without
copying the 45 case databases into Git.

Focused gate: `20_sqlite_template.test.py` 6/6,
`17_sqlite_trigger_capabilities.py` 10/10, `25_sqlite_ivm.test.py` 5/5, and
`23_crossover.test.mjs` 3/3, totaling 24/24 test methods. No CI workflow was
added or changed.

## Remaining implementation gaps

- SQL-text parsing and a stock-SQLite loadable extension API.
- Arithmetic signed-delta COUNT/SUM maintenance and statement-level batching.
- Shared pg_ivm/DD scenario adapters for the additional self/multiway,
  projection-bag, global-aggregate, and rejection cases.
- Nullable SQL semantics, text/collation, floating point and additional
  aggregates/operators listed above.
- Concurrent-writer scheduling tests, crash/power-loss recovery, hostile schema
  mutation detection, and migration/version upgrade behavior.

