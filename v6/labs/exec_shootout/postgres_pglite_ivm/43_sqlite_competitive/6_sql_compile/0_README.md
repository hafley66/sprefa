# SELECT lowering boundary

Reusable unit: Take 1's pinned `sqlite3-parser = 0.17.0` (Unlicense), including
`Parser`, `Select`/`FromClause` AST, and `Expr::Display` SQL serialization.
`serde_json = 1.0.149` (MIT/Apache-2.0) serializes the bounded plan. This isolated
binary compiles one SELECT from stdin; it never opens a database or maintains data.
No OpenIVM/DuckDB plan dependencies are required for this smallest target.

`compile(sql: &str) -> Result<Value, String>` parses one SELECT, admits a bounded
bag fragment, preserves source aliases, and emits occurrence-to-source mapping,
predicate and two projections. The extension privately binds scalar expressions;
`take2_attach` validates actual source schema before any source DML is admitted.
AST shape errors fail in the compiler; scalar binding errors fail at vtab creation.

Initial fragment: two expressions over one to three ordinary main table occurrences,
INNER/CROSS/comma joins with ON and WHERE. Repeated source names share one delta
side. Only columns k/v are admitted by the extension scalar binder. Source tables
must have id/k/v INTEGER columns and be empty at attachment. CTEs, derived tables,
USING/NATURAL/outer joins, aggregation, DISTINCT, windows, compounds, sorting and
limits are explicit unsupported shapes in this compiler target. Existing dedicated
aggregate, negation, reach and frontier modes retain their separate APIs.

Build with task-owned CARGO_HOME and CARGO_TARGET_DIR, then pipe SELECT text into
`take2-sql-compile`. Output is JSON containing `plan`, `sources`, and `create_sql`
for a virtual table named `compiled_result` using `take2_lazy(plan, '<json>')`.
