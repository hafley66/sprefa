# OpenIVM and local compiler reuse boundary

Read after [1_sqlite_boundaries.md](1_sqlite_boundaries.md). Local order: `/tmp/sprefa-sqlite-extension-research.ZsALrZ/openivm/CMakeLists.txt`, `src/delta/delta_compiler.cpp`, `src/delta/operators/dispatch.cpp`, then `docs/limitations.md`; local implementation: `../25_sqlite_ivm/0_README.md`, `src/0_types.rs`, `src/2_lower.rs`, `../21_sqlite_template_reuse.md`.

## OpenIVM code boundary

OpenIVM is compiled as a DuckDB extension. Its CMake source list directly includes DuckDB extension build macros and code that names DuckDB `OptimizerExtensionInput`, `Connection`, `LogicalOperator`, `LogicalGet`, catalog/client data, and DuckLake/LPTS sources ([`CMakeLists.txt`](https://github.com/ila/openivm/blob/3b3938f4f8293875b56157f563c6f8cb196a0b41/CMakeLists.txt), [`delta_compiler.cpp`](https://github.com/ila/openivm/blob/3b3938f4f8293875b56157f563c6f8cb196a0b41/src/delta/delta_compiler.cpp)). Its parser/plan stages consume DuckDB-bound logical plans and compile SQL refresh fragments, not a portable SQL AST-to-SQLite compiler.

| Layer | Pinned implementation | SQLite reuse status |
|---|---|---|
| SQL parser/binder | DuckDB parser, catalog, bound/optimized logical plan; LPTS serializes plan SQL | Engine-bound. |
| Model/classification | `DeltaViewModel`, `IncrementalChecker`, refresh type / feature analysis | Concepts and tests are readable; C++ objects depend on DuckDB plans. |
| Delta compiler | `DeltaCompiler` dispatches model nodes into scans, filters, projection, aggregate, joins, DISTINCT, windows, CTEs, etc. | Generated expressions are DuckDB SQL and catalog contracts. |
| State and apply | `openivm_data_*`, `openivm_delta_*`, metadata, `MERGE`, refresh locks/daemon | Requires a port of persistent schema and SQL operations. SQLite has no `MERGE` statement or DuckDB daemon API. |
| Scheduling | `PRAGMA refresh`, metadata plus separate-connection background daemon | Separate from delta compilation; no SQLite equivalent is supplied. |

`dispatch.cpp` names strategies including `JOIN_INCLUSION_EXCLUSION`, `JOIN_REGULAR_N_TERM`, `AGGREGATE_GROUP_BY_MULTIPLICITY`, `DISTINCT_COUNT_AGGREGATE`, `ASOF_AFFECTED_RECOMPUTE`, and global recomputes ([source](https://github.com/ila/openivm/blob/3b3938f4f8293875b56157f563c6f8cb196a0b41/src/delta/operators/dispatch.cpp)). The operator set proves neither a uniform algebraic delta path nor a SQLite implementation.

## Operator and semantic evidence

| SQL shape | OpenIVM current maintenance class | Evidence / test family |
|---|---|---|
| projection/filter, `UNION ALL`, inner/cross joins | Algebraic delta strategies, including inclusion-exclusion or eligible regular N-term joins | [`dispatch.cpp`](https://github.com/ila/openivm/blob/3b3938f4f8293875b56157f563c6f8cb196a0b41/src/delta/operators/dispatch.cpp), [`inner_join.test`](https://github.com/ila/openivm/blob/3b3938f4f8293875b56157f563c6f8cb196a0b41/test/sql/inner_join.test) |
| `SUM`, `COUNT`, `AVG`, variance | Delta MERGE; AVG and variance have hidden decomposed state | [`limitations.md`](https://github.com/ila/openivm/blob/3b3938f4f8293875b56157f563c6f8cb196a0b41/docs/limitations.md) |
| `MIN`, `MAX`, boolean and non-summable output | Insert-only shortcut or affected-group recompute | [`limitations.md`](https://github.com/ila/openivm/blob/3b3938f4f8293875b56157f563c6f8cb196a0b41/docs/limitations.md) |
| DISTINCT | `DISTINCT_COUNT_AGGREGATE`; some inner-DISTINCT forms default to group recompute; optional aux state only for a documented narrow shape | [`distinct.test`](https://github.com/ila/openivm/blob/3b3938f4f8293875b56157f563c6f8cb196a0b41/test/sql/distinct.test), [limits](https://github.com/ila/openivm/blob/3b3938f4f8293875b56157f563c6f8cb196a0b41/docs/limitations.md) |
| left/right/full outer joins | MERGE plus targeted/group recompute depending on shape/settings | [`full_outer_join.test`](https://github.com/ila/openivm/blob/3b3938f4f8293875b56157f563c6f8cb196a0b41/test/sql/full_outer_join.test) |
| scalar correlated/LATERAL | affected-key group recompute | [`limitations.md`](https://github.com/ila/openivm/blob/3b3938f4f8293875b56157f563c6f8cb196a0b41/docs/limitations.md) |
| windows and top-k | partition-level or affected-key recompute; deterministic ordering required | [`window.test`](https://github.com/ila/openivm/blob/3b3938f4f8293875b56157f563c6f8cb196a0b41/test/sql/window.test), [`limitations.md`](https://github.com/ila/openivm/blob/3b3938f4f8293875b56157f563c6f8cb196a0b41/docs/limitations.md) |
| recursive CTE | full refresh | [`recursive_rewriter.test`](https://github.com/ila/openivm/blob/3b3938f4f8293875b56157f563c6f8cb196a0b41/test/sql/recursive_rewriter.test), [`limitations.md`](https://github.com/ila/openivm/blob/3b3938f4f8293875b56157f563c6f8cb196a0b41/docs/limitations.md) |

SQL bag and NULL semantics are delegated to DuckDB plans and SQL execution. The test suite has named coverage for DISTINCT, outer joins, recursive rewriter, and transactional lifecycle, but it does not establish matching SQLite semantics. `COUNT(DISTINCT ...)` routes to full refresh in the cited limitations document. Multiway joins use inclusion-exclusion/N-term plan rules, while affected-key recomputation remains present for several other shapes.

pg_ivm supplies a distinct contrast: PostgreSQL statement-level transition tables are preserved in top-transaction memory, then source query RTEs are rewritten through pre-state, transition deltas, post-state, DISTINCT/aggregate, and outer-join rewrites ([`matview.c:1277-1417`](https://github.com/sraoss/pg_ivm/blob/dda7470e085822c215411c0b349f4aa4fbb9fdf4/matview.c#L1277-L1417)). SQLite ordinary triggers do not expose statement transition relations.

## Existing local `25_sqlite_ivm`

The local implementation accepts a deliberately small plan representation, validates named two-table inner join/count/sum domains, creates ordinary result/support tables and persistent row triggers, and maintains state during source writes. It does not parse an arbitrary SQL SELECT. The detailed existing inventory and executed scope is [`21_sqlite_template_reuse.md`](../21_sqlite_template_reuse.md).

| Existing local artifact | Current role | Retainable evidence boundary |
|---|---|---|
| `25_sqlite_ivm/src/0_types.rs` | plan / catalog input types | typed input contracts and rejection surface |
| `25_sqlite_ivm/src/2_lower.rs` | DDL, trigger SQL, delta/result maintenance | persistent SQLite trigger transport for the covered plans |
| `19_sqlite_template_adapter.py` | executes compiler-emitted aggregate SQL in persistent triggers | affected-group SQL template consumer |
| `20_sqlite_template.test.py`, `27_sqlite_ivm.test.py` | deterministic local tests | coverage for their stated source/query families |
| DD circuit files (`30` through `40`) | separate semantic/oracle arms | no evidence that the SQLite plugin accepts or schedules those circuits |

The portable unit available for later review is the local compiler's emitted SQL template for the stated COUNT/SUM inner-join family, plus its schema/trigger installer pattern. OpenIVM's C++ compiler cannot be lifted without replacing DuckDB parse/bind/logical-plan/optimizer/executor APIs, DuckDB SQL dialect operations, refresh metadata, and scheduling.
