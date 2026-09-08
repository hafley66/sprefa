# Source and reuse ledger

Read-only research clones: `/tmp/sprefa-sqlite-extension-research.ZsALrZ/`.
Build archive and outputs: `/tmp/sprefa-sqlite-native-take2/`.

| Source | Pin | License | Inspected unit |
|---|---|---|---|
| SQLite | f3b9f74d81132426dee1ccc07a67fdad2ccfeaa9 | Public domain | src/vtab.c; ext/fts5/fts5_main.c transaction and savepoint callbacks; fts5_storage.c prepared SQL, shadow DDL, sync/rollback; fts5savepoint.test and fts5conflict.test |
| OpenIVM | 3b3938f4f8293875b56157f563c6f8cb196a0b41 | MIT | delta_compiler.cpp, operators/dispatch.cpp, test/sql/inner_join.test |
| pg_ivm | dda7470e085822c215411c0b349f4aa4fbb9fdf4 | PostgreSQL | matview.c transition-table capture, pre-state/delta/post-state rewrite and apply |

The extension directly uses SQLite's supplied sqlite3ext.h C ABI dispatch table.
No upstream implementation source is copied. FTS5's shadow-write and lifecycle
mechanisms inform the boundary. Current OpenIVM compilation consumes DuckDB
logical plans, catalog/client objects and SQL. No ready SQLite port is established.

Research correction: [discussion 309, January 8 reply](https://github.com/vlcn-io/cr-sqlite/discussions/309)
proposes feeding SQLite hook events to Feldera-generated Rust. The replies were
read on 2026-09-08. This is a proposal; the source-research report's statement that
the discussion does not name Feldera is incorrect. No implementation or lifecycle
test pass follows from that proposal.

Terra's original Tcl build failure remains a failed attempt. This task found
existing `/opt/homebrew/opt/tcl-tk@8/lib/tclConfig.sh` and built a separate SQLite
archive without modifying the research clones or global packages. Individually
executed fts5savepoint.test: 3 tests, 0 errors; fts5conflict.test: 27 tests, 0 errors.

## Concrete compiler reuse boundary

The compiling unit is `0b_delta.h::contribute(Tab*,sqlite3_value**,int,int)`, linked
into `1_native.c` against the public SQLite extension ABI. It adapts Take 1's
`25_sqlite_ivm/src/2_lower.rs` signed count/sum support accumulator to xUpdate.
Source OLD/NEW binding and state application are separate from lifecycle callbacks.
SQLite prepare/bind supplies SQL syntax/name resolution; this experiment selects
an explicit mode rather than accepting a general SELECT string.

OpenIVM `DeltaCompiler` depends on `OptimizerExtensionInput`, DuckDB `Connection`,
`LogicalOperator`, `DeltaViewModel`, catalog/client and refresh metadata. Its
dispatch selects filter/projection/aggregate/join strategies; join.cpp constructs
inclusion-exclusion terms. A literal port would also require its bound expression
and SQL serialization machinery and SQLite replacements for MERGE/refresh state.
No such compiler port was undertaken. The smallest compiling target above needs
SQLite headers and a C compiler only. Its query oracles follow inner_join.test
and aggregate.test scenarios, and Take 1's accumulator pattern. pg_ivm's per-RTE
pre/delta/post-state sequence informed the sequential multi-source ordering.
These are adaptations of techniques; upstream source is not copied or linked.
