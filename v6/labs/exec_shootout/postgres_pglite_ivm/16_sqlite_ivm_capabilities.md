# Stock SQLite SQL-facing IVM capability reference

Research date: 2026-09-08. Scope: a loadable C/Rust extension for stock SQLite,
SQL query declaration, ordinary base-table writes, automatic maintained ordinary
result relation. No alternate database or consumer-side dataflow language.

Status: no qualifying ready-to-run extension was verified in this search. This is
a bounded search result, not evidence that none exists. A SQL-to-trigger compiler
slice has been proposed for approval; no extension or dependency was added.

## Evidence index

1. Existing candidates and their actual interfaces.
2. PostgreSQL internal dependencies versus SQLite public interfaces.
3. Reusable SQLite parsers and unresolved semantic work.
4. Executed SQLite mechanism tests.
5. Proposed, unimplemented acceptance slice and filesystem handoff.

## Candidate matrix

Evidence labels: **source** means inspected upstream code; **docs** means an
upstream claim; **executed** applies only to the local mechanism tests below.

| Candidate | Consumer interface and result | Incremental / automatic ordinary writes | Status for this request |
|---|---|---|---|
| [sqlite-mview](https://github.com/dilipvamsi/sqlite-mview) | C extension, SQL `mview_create(name, query)`, physical table in attached DB | Explicit `mview_refresh`; full query materialization | SQL-facing stock extension, full-refresh boundary |
| [AhmedFaizanDev/ivm](https://github.com/AhmedFaizanDev/ivm) | Python `Engine.add_sql_view`, Python result Z-set | SQLite row-log or session capture, explicit Python `SqliteAdapter.flush()` | Host-runtime integration required |
| [Rindle](https://rindle.sh/docs/how-it-works?path=engine) | Open Rust query AST/builder, graph, in-memory view | Open engine requires push/flush; commercial SQL capture uses one controlled writer | Open API differs from requested SQL extension |
| [stepping](https://github.com/leontrolski/stepping) | Python backend IVM API, SQLite/Postgres storage | Python computation/iteration interface | Consumer-language boundary |
| [Turso](https://github.com/tursodatabase/turso/blob/main/COMPAT.md) | SQL materialized views | Experimental IVM in rewritten database | Engine replacement; SQLite extension ABI differs |
| [OpenIVM](https://github.com/ila/openivm) | SQL materialized views and incremental refresh | DuckDB extension, including refresh scheduling | Different database; useful SQL-to-SQL prior art |
| [SynQLite relation-view thesis](https://munin.uit.no/handle/10037/30400) | 2023 research on SQLite CRR materialized views | Thesis claims incremental maintenance and replicated provenance | Source/package/API/release not recovered; unresolved candidate |
| [pg_ivm](https://github.com/sraoss/pg_ivm) | SQL `pgivm.create_immv(name, query)`, ordinary relation | Generated PG triggers, immediate maintenance | Reference behavior; PG server internals required |

### Closest stock extension: sqlite-mview

Verified release [v3.0.0, 2026-02-07](https://github.com/dilipvamsi/sqlite-mview/releases/tag/v3.0.0),
MIT; inspected commit `e83dc6c958884667d6e17b2e0042c47a3fe3782c`.
`mview_attach` selects an attached cache file; `mview_init` creates metadata;
`mview_create` stores the SQL and materializes it. Source
[`mview_extension.c`](https://github.com/dilipvamsi/sqlite-mview/blob/e83dc6c958884667d6e17b2e0042c47a3fe3782c/mview_extension.c#L557)
implements refresh by creating a complete shadow table from the source query,
then replacing the old table under a savepoint. Ordinary writes do not invoke
this refresh. NULL, duplicates, joins and aggregates are evaluated by SQLite
when refresh runs. Persistence/concurrency/rollback claims were not executed.
No acceptance benchmark adapter was added because incremental maintenance is
explicitly absent in its README.

### Closest SQL compiler with SQLite capture: Python ivm

Source version `0.1.0`, alpha, MIT; commit
[`ca5119c9053dd08b4f159258ef952b8fba871442`](https://github.com/AhmedFaizanDev/ivm/tree/ca5119c9053dd08b4f159258ef952b8fba871442),
2026-07-05. README says not yet published to PyPI.

| Concern | Inspected evidence |
|---|---|
| Algebra | `ivm/sql.py`: projection/filter, inner/left/right/full equijoins, GROUP BY, COUNT/SUM/AVG/MIN/MAX, DISTINCT/HAVING; custom Python parser |
| Delete / bag / NULL | Weighted Z-sets and OLD/NEW capture; SQL parser recognizes NULL and IS NULL. Full SQL equivalence not executed here |
| Automatic SQL writes | `ivm/adapters/sqlite.py:flush` must drain the log and call the Python engine |
| Transactions | Logs reside in SQLite; Python engine state changes in `flush` separately. Transactional rollback coupling was not verified |
| Persistence/reopen | Captured logs persist; no ordinary SQL maintained result relation in the inspected adapter |
| Multiple connections | Persistent trigger logs capture other writers; recursive-trigger setting is applied only to the adapter connection. Session capture is connection-local |
| Tests / maintenance | Upstream tests cover REPLACE, WITHOUT ROWID, values and seeded writes; inspected tests explicitly call flush. Issue listing returned zero entries on research date |

Sources: [adapter](https://github.com/AhmedFaizanDev/ivm/blob/ca5119c9053dd08b4f159258ef952b8fba871442/ivm/adapters/sqlite.py),
[SQL compiler](https://github.com/AhmedFaizanDev/ivm/blob/ca5119c9053dd08b4f159258ef952b8fba871442/ivm/sql.py),
[adapter tests](https://github.com/AhmedFaizanDev/ivm/blob/ca5119c9053dd08b4f159258ef952b8fba871442/tests/test_sqlite_adapter.py),
[package metadata](https://github.com/AhmedFaizanDev/ivm/blob/ca5119c9053dd08b4f159258ef952b8fba871442/pyproject.toml).

Rindle's open engine is Apache-2.0 according to its docs. Its view lifecycle is
AST/build/hydrate/push/flush; commercial capture is distinguished in the same
[lifecycle page](https://rindle.sh/docs/how-it-works?path=engine). No Rindle
release, commercial transaction contract, or package was executed. Stepping's
source manifest reports `0.0.3`, MIT; repository last push was 2024-01-11, which
does not establish a current package release. The SynQLite thesis is by Lars
Marius Elvenes, dated 2023-06-01; the repository record remains available but its
PDF redirected to an inaccessible JavaScript archive. Do not infer its SQL
subset or source-code license from the thesis abstract/license.

## pg_ivm: reusable concepts and PG-specific implementation

Verified latest release [v1.15, 2026-06-30](https://github.com/sraoss/pg_ivm/releases/tag/v1.15),
PostgreSQL license, commit `377a37dc72a922486d9d3d0c8caf2f7e91900c93`.
The inspected main README also contains later development changes, so its
capability list must not automatically be attributed to the v1.15 binary.

| Source seam | Inspected implementation | SQLite public counterpart / missing layer |
|---|---|---|
| [`pg_ivm.c:create_immv`](https://github.com/sraoss/pg_ivm/blob/main/pg_ivm.c#L176) | `pg_parse_query`, `SelectStmt`, `CreateTableAsStmt`, `transformStmt`, `ExecCreateImmv` | Scalar extension function plus separate parser; SQLite prepare validates SQL but does not expose PG-like analyzed Query objects |
| [`createas.c`](https://github.com/sraoss/pg_ivm/blob/main/createas.c) | `DefineRelation`, `CreateTrigger`, catalog dependencies, index definitions | Public SQL CREATE TABLE/INDEX/TRIGGER generated by extension |
| [`matview.c`](https://github.com/sraoss/pg_ivm/blob/main/matview.c) | SPI execution, OLD/NEW transition tuplestores, query rewrites, MVCC snapshots, relation locks | SQLite row OLD/NEW expressions and SQL execution; no equivalent public statement transition-table/MVCC API |
| [`Makefile`](https://github.com/sraoss/pg_ivm/blob/main/Makefile) | PGXS; createas/matview/pg_ivm/ruleutils/subselect modules | Stock SQLite loadable-extension ABI is a separate integration target |

Reusable algorithmic ideas include signed join deltas, support multiplicities,
aggregate auxiliary counts/sums, and transactional maintenance. The referenced C
modules directly depend on PostgreSQL ASTs, catalogs, executors and locking.
Porting their integration requires SQLite-specific binding, delta lowering,
schema lifecycle and transaction handling.

Current upstream [restriction documentation](https://github.com/sraoss/pg_ivm#supported-view-definitions-and-restriction)
lists inner/self joins, restricted outer joins, DISTINCT, five aggregates,
restricted subqueries/CTEs. It excludes window functions, HAVING, ORDER/LIMIT,
set operations and other shapes. Floating SUM/AVG can differ from recomputation;
the README warns about this explicitly. Recursion is outside this proposed
nonrecursive comparison.

## SQLite extension interfaces and constraints

- [Loadable extensions](https://sqlite.org/loadext.html) can register SQL
  functions and virtual-table modules. A SQL installer can generate schema;
  the executed callback tests below establish a bounded mechanism result.
- [Application functions](https://sqlite.org/appfunc.html) are registered per
  connection. Installer functions need direct-only/security review; a function
  called by a persistent trigger creates a writer connection setup dependency.
- [Update hooks](https://sqlite.org/c3ref/update_hook.html) are per connection,
  omit WITHOUT ROWID and some delete paths, and cannot prepare/step SQL in the
  invoking callback. [Commit/rollback hooks](https://sqlite.org/c3ref/commit_hook.html)
  likewise prohibit reentrant SQL. They cannot alone provide the requested
  automatic transactional maintained relation across ordinary connections.
- [Preupdate hooks](https://sqlite.org/c3ref/preupdate_blobwrite.html) expose
  old/new values but require a compile-time option and per-connection
  registration. They exclude virtual/system tables. They do not supply a query
  compiler or maintained relation.
- [Triggers](https://sqlite.org/lang_createtrigger.html) are row-level only.
  Persistent triggers can cover other connections; TEMP triggers cannot.
  Trigger DML uses unqualified target names and same-database tables. Direct
  CTE statements, DML ORDER/LIMIT and several other forms are restricted.
- [Conflict behavior](https://sqlite.org/lang_conflict.html) matters to generated
  maintenance: REPLACE deletes fire delete triggers only with recursive triggers
  enabled; outer conflict policy overrides a trigger body's OR policy. FAIL can
  retain preceding row changes, while ABORT rolls back the current statement.

WAL/rollback durability follows the selected SQLite journal/synchronous settings.
A persistent trigger-based design still needs explicit contracts for schema
changes, direct edits of result/support tables, disabled triggers, writable_schema,
incremental blob I/O, virtual tables and attached databases. None is implicitly
covered by ordinary INSERT/UPDATE/DELETE acceptance.

## Reusable parsers

| Library | Verified state | Available interface / limits |
|---|---|---|
| [sqliteai/liteparser](https://github.com/sqliteai/liteparser) | MIT; release [1.0.0, 2026-04-28](https://github.com/sqliteai/liteparser/releases/tag/1.0.0); source commit `b19952d47dbbe6e93813a29c6e416e1d33c26caa` dated 2026-03-11 | C SQLite-derived Lemon grammar, arena AST, visitors, mutation and unparse. `src/liteparser.h`; no external library dependencies claimed. Upstream tests not executed |
| [gwenn/lemon-rs, sqlite3-parser](https://github.com/gwenn/lemon-rs) | [crate 0.17.0](https://docs.rs/crate/sqlite3-parser/latest), 2026-06-30; crate manifest Apache-2.0/MIT | Rust SQLite-derived parser/AST. Repository API labels Unlicense, but crate manifest declares Apache-2.0/MIT; retain license-file audit if selected |

Parser acceptance does not establish semantic binding or maintainability. The
extension must still resolve columns/catalogs, reject unsupported AST nodes,
preserve SQLite affinity/collation/NULL behavior, generate deltas and validate
the result SQL with SQLite. No parser dependency was selected or installed.

Liteparser's current issue listing returned zero entries. Lemon-rs
[issue 100](https://github.com/gwenn/lemon-rs/issues/100) remains open for lexer
differences involving comments, variables, BOM and numeric tokens.
[PR 112](https://github.com/gwenn/lemon-rs/pull/112) remains open for optional
parser feature fixes. These are reported issues/proposals, not locally verified
parser failures or shipped fixes.

## Executed capability probes

Run from this directory:

```sh
python3 17_sqlite_trigger_capabilities.py -v
```

10 tests passed on stdlib-linked SQLite **3.53.2**, 2026-09-08. Hand-authored event
triggers are mechanism fixtures, with no IVM compiler claim. The system SQLite
CLI **3.43.2** independently reproduced REPLACE event omission, rollback and the
trusted-schema guard restriction.

| Tested mechanism | Observed result |
|---|---|
| REPLACE, recursive triggers OFF | Delete event omitted; only new insert captured |
| REPLACE ON; UPSERT | OLD delete plus NEW insert; NULL value preserved |
| Transaction/savepoint rollback | Base rows and event rows restored together |
| Statement constraint ABORT | Earlier rows/events from the statement undone |
| Second connection; WAL; close/reopen | Persistent triggers and committed rows remain effective |
| SQL guard on recursive-triggers setting | OFF rejected before base/event changes |
| Same guard with trusted_schema OFF | Write rejected as unsafe pragma virtual-table use |
| Outer OR REPLACE with inner OR IGNORE | Outer policy replaced a conflicting trigger target row |
| Scalar callback DDL then outer ROLLBACK | Created table and population removed |
| Callback error during autocommit setup | Partial schema remained without savepoint; explicit savepoint cleanup removed it |

Accounting: 10 mechanism tests passed, 0 failed, 0 skipped. Zero IVM extension
acceptance tests or candidate benchmarks executed in this pass. Crash/power-loss,
simultaneous competing writers and full SQL semantic coverage remain untested.
Coverage added: runnable local build-independent tests; no CI workflow changed.

## Proposed slice, awaiting boundary approval

Proposed API, not an installed function:

```sql
SELECT sqlite_ivm_create('totals', '
  SELECT dimension.group_id,
         COUNT(*) AS row_count,
         SUM(fact.amount * dimension.factor) AS weighted_sum
  FROM fact JOIN dimension USING(group_id)
  GROUP BY dimension.group_id
');
-- Then ordinary INSERT, UPDATE, DELETE against fact/dimension.
SELECT * FROM totals;
```

Use the existing [9_crossover_workload.mjs](9_crossover_workload.mjs) input/oracle,
mutation phases and nonrecursive fixture. A later opt-in adapter belongs in
the existing crossover runner and reporting pipeline. Fresh exact recomputation
and input/output checks must remain outside the matched timed mutation plus
maintenance/materialization/count boundary. No recursion benchmark substitutes
for this nonrecursive target.

Proposed first subset: main-schema ordinary tables; explicit columns/aliases;
inner equijoin; integer GROUP BY keys; COUNT and bounded integer SUM expressions.
NULL grouping, all-NULL sums, duplicate join support and deletion of the last
contribution require exact tests. Integer storage classes, expression overflow
and aggregate intermediate overflow require enforced admission rules, since
SQLite affinity alone does not guarantee integer values. Reject unsupported
recursion, outer/self joins, windows, subqueries, nondeterminism, unreviewed
collations and floating aggregates explicitly at install time.

Proposed implementation sequence: existing parser AST -> SQLite catalog/binding
checks -> accepted algebra -> persistent SQL delta triggers/support tables ->
ordinary result table. Install under a savepoint, preserve caller transactions,
and roll back every created object on error. Initial population is full query
evaluation; later supported writes must use deltas without hidden full refresh.

Writer setup decision remains open: pure-SQL pragma guard requires compatible
trusted_schema settings, or a reviewed native guard requires loading the
extension on every writer and fails closed if absent. Test ordinary writes,
REPLACE/UPSERT/IGNORE/FAIL/ABORT, rollback, reopen and a second unconfigured
connection. Result/support-table protection and schema migration are explicit
contracts to resolve before treating a slice as general SQL IVM.

## Actual filesystem

All new files belong to the isolated worktree, not the main checkout:

`/Users/chrishafley/projects/sprefa/.boop-worktrees/feature/postgres-ivm-crossover/`

| Path relative to worktree | Role and lifetime |
|---|---|
| `v6/labs/exec_shootout/postgres_pglite_ivm/16_sqlite_ivm_capabilities.md` | Source-controlled research/proposal; read first |
| `v6/labs/exec_shootout/postgres_pglite_ivm/17_sqlite_trigger_capabilities.py` | Source-controlled capability tests; Python opens SQLite connections and issues fixture SQL |
| `v6/labs/exec_shootout/postgres_pglite_ivm/results/sqlite-capabilities-20260908/` | New retained test receipt only; earlier receipts untouched |
| OS temporary `sqlite-ivm-capabilities-*/probe.sqlite` | Per-test generated SQLite DB; SQLite writes schema/rows, and WAL/SHM sidecars during the WAL test; closed and removed by test cleanup |

No extension source/binary, dependency checkout, installed extension, retained
test database or global configuration was created in this pass. The test verifies
that schema/triggers/data survive its close/reopen step before temporary cleanup.

## Research boundaries and documentation inventory

Primary sources only. Inspected SQLite loadable-extension, scalar function,
update/preupdate/commit hook, trigger and conflict pages; candidate READMEs,
package metadata, adapter/compiler/refresh source, release metadata and relevant
issues listed above. Parser tests and candidate test suites were not run.
SynQLite source retrieval remains unresolved. Search results, including the
[2023 SQLite forum question](https://sqlite.org/forum/forumpost/c7437e2f43),
do not establish present-day absence. No performance ranking or architecture-wide
correctness proof follows from these mechanism probes.
