# SELECT lowering boundary

Reusable unit: Take 1's pinned `sqlite3-parser = 0.17.0` (Unlicense), including
`Parser`, `Select`/`FromClause` AST, and `Expr::Display` SQL serialization.
`serde_json = 1.0.149` (MIT/Apache-2.0) serializes the bounded plan. This isolated
binary compiles one SELECT from stdin; it never opens a database or maintains data.
No OpenIVM/DuckDB plan dependencies are required for this smallest target.

`compile(sql: &str) -> Result<Value, String>` parses one SELECT, admits a bounded
bag fragment, preserves source aliases, and emits occurrence-to-source mapping,
predicate and projections. The extension privately binds scalar expressions;
`take2_attach` requires empty ordinary sources and installs forwarding triggers.
Actual row domains are checked when forwarded; scalar binding uses canonical k/v.
AST shape errors fail in the compiler; scalar binding errors fail at vtab creation.

Admitted bag fragment: two expressions over one to three ordinary main table occurrences,
INNER/CROSS/comma joins with ON and WHERE. Repeated source names share one delta
side. Only columns k/v are admitted by the extension scalar binder. Source tables
must have id/k/v INTEGER columns and be empty at attachment. CTEs, derived tables,
USING/NATURAL/outer joins, DISTINCT, windows, compounds, sorting and
limits are explicit unsupported shapes in this compiler target. Existing dedicated
aggregate, negation, reach and frontier modes retain their separate APIs.

Grouped fragment: `SELECT key,COUNT(*),SUM(value) ... GROUP BY key`, using the
same one-to-three occurrences and predicates. GROUP BY must repeat the one key
AST, and COUNT/SUM admit no DISTINCT/FILTER/ORDER clauses. The serialized plan
sets `aggregate=true`; the C executor uses separate support count, integer sum,
and non-NULL count, with exact empty-group removal. Key and SUM argument must
produce INTEGER/NULL before aggregation. Other aggregate shapes remain errors.
General grouped source SQL is exercised by `11_group_compile_test.py` and the
shared `sqlite-competitive-compiled` arm (five admitted core circuits).

Build with task-owned CARGO_HOME and CARGO_TARGET_DIR, then pipe SELECT text into
`take2-sql-compile`. Output is JSON containing `plan`, `sources`, and `create_sql`
for a virtual table named `compiled_result` using `take2_lazy(plan, '<json>')`.
