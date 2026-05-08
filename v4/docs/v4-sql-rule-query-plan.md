# V4 SQL Rule Query Plan

## Status

This document is target design plus the first executable slice. Current v4 already has rule declaration/write behavior, `FactRead`, `FactRead::anti`, LSP diagnostic ops, CST/LSP substrate, and a batch-local `sql`` operator.

Implemented `sql`` status:

```text
upstream cursor batch -> temp input relation -> in-memory SQLite query -> output cursors
```

The first slice snapshots referenced fact tables through `FactStore::rows_of` into the per-batch SQLite connection. That keeps the implementation trait-backed while rule/query semantics stay SQLite-shaped.

The goal is to make rule querying SQLite-shaped without expanding the host language into a second SQL.

```text
current cursor batch -> temp input relation -> SQLite query -> output cursors -> generation commit
```

## Current State

Implemented pieces:

```text
rule(:name, A?, B?)       declares a rule relation
rule(:name) { ... }       writes rows to that relation
... > rule(:name)         sink-position write
FactRead                  per-cursor relation read
FactRead::anti            runtime anti-join primitive
sql`...`                  batch-local SQLite relation op
FactStore::declared_cols  declared table column metadata for SQL
lsp_error/lsp_warn/...    runtime diagnostic ops
cst::DslBodyLsp           body-level LSP hooks for DSLs
```

Missing pieces:

```text
rule schema metadata usable by SQL/LSP
logical rule name -> physical table name rewrite
direct execution against SqliteFactStore physical tables
recursive CTE table detection
SQL diagnostics surfaced back into document-level LSP state
SQL LSP metadata for tables, columns, and input terms
```

## SQL DSL Contract

`sql`` body is SQLite syntax.

Use SQLite words directly:

```sql
SELECT
FROM
JOIN
WHERE
NOT EXISTS
WITH RECURSIVE
GROUP BY
ORDER BY
LIMIT
```

Special names:

| SQL name | Meaning |
| --- | --- |
| `input` | current upstream cursor batch as a temp relation |
| current file rule name | logical relation from `rule(:name, ...)` |
| future `some_file.rule_name` | cross-file rule relation |
| `_strings`, `_refs`, `_files` | core store tables if exposed |

Physical store table names are implementation detail. SQL authors write logical names.

## Input Relation

Every `sql`` op sees the current dispatch batch as `input`.

Minimum columns:

```text
__cursor_idx
value
one column per current cursor term
```

Later columns:

```text
ref / at
repo
rev
file
lo
hi
```

`__cursor_idx` is the stable row identity for mapping SQL output rows back to the source cursor in the current batch.

## Output Cursor Mapping

SQL result rows become cursors.

Rules:

| Result shape | Cursor behavior |
| --- | --- |
| includes `__cursor_idx` | clone that input cursor |
| omits `__cursor_idx` | emit a synthetic cursor from the row |
| selected column `value` | replace `cursor.value` |
| selected column `OP` | set output term `OP` |
| selected alias `OP AS HOOK_OP` | set output term `HOOK_OP` |

Default rule: selected columns become output terms by their result column name.

## Interpolation

Interpolation is for values and current input columns, not identifiers.

Target forms:

| Form | Lowering |
| --- | --- |
| `${OP}` | `"input"."OP"` |
| `${&.value}` | `"input"."value"` |

Rejected forms:

```sprf
sql`SELECT * FROM ${TABLE}`
sql`SELECT ${COLUMN} FROM frontend_hooks`
```

Reason: SQLite bind parameters cannot bind identifiers. Identifier interpolation would make SQL string building, prepare caching, and LSP metadata unreliable.

Scalar external parameters can be added later through explicit source relations:

```text
env(...)
config(...)
```

## Rule Table Name Resolution

Inside a `.sprf` file:

```sprf
rule(:frontend_hooks, OP?, REF?) { ... }
```

SQL author writes:

```sql
SELECT OP, REF FROM frontend_hooks
```

Lowering rewrites the logical name to the physical table name for the current file namespace.

```text
frontend_hooks -> <current_file_namespace>__frontend_hooks_facts
```

Exact physical naming remains private to the store/lowering layer.

Future cross-file shape:

```sql
SELECT * FROM api_rules.openapi_ops
```

The first implementation can reject qualified rule names until imports/namespaces exist.

## Batch Execution

`sql`` runs once per runtime dispatch batch at that op.

```text
1. collect current cursor batch
2. create/populate temp input relation
3. rewrite logical rule names to physical table names
4. rewrite `${TERM}` input-column references
5. prepare/execute SQLite query
6. map SQL result rows back to cursors
7. emit output cursor batch
```

Generation/tick remains the consistency and commit boundary. `sql`` does not automatically buffer every cursor in a full generation.

Whole-generation queries require a later explicit barrier:

```sprf
... > barrier > sql`...`
```

or:

```sprf
... > collect(:generation) > sql`...`
```

## Example Queries

### Inner Join

```sprf
openapi_ops(OP: OP?)
> sql`
    SELECT input.__cursor_idx, input.OP, hooks.REF
    FROM input
    JOIN frontend_hooks AS hooks
      ON hooks.OP = ${OP}
  `
```

Lowered SQL shape:

```sql
SELECT input.__cursor_idx, input.OP, hooks.REF
FROM input
JOIN <current_file>__frontend_hooks_facts AS hooks
  ON hooks.OP = input.OP;
```

### Anti-Join / Missing

```sprf
rule(:missing_frontend_hooks, OP?) {
  openapi_ops(OP: OP?)
  > sql`
      SELECT input.__cursor_idx, input.OP
      FROM input
      WHERE NOT EXISTS (
        SELECT 1
        FROM frontend_hooks
        WHERE frontend_hooks.OP = ${OP}
      )
    `
  > lsp_warn(:missing_frontend_hook)`missing frontend hook for ${OP}`
}
```

Lowered SQL shape:

```sql
SELECT input.__cursor_idx, input.OP
FROM input
WHERE NOT EXISTS (
  SELECT 1
  FROM <current_file>__frontend_hooks_facts AS frontend_hooks
  WHERE frontend_hooks.OP = input.OP
);
```

### Recursive Blast Radius

```sprf
sql`
  WITH RECURSIVE radius(node, depth) AS (
    SELECT ${SYMBOL}, 0
    UNION
    SELECT edges.to, radius.depth + 1
    FROM edges
    JOIN radius ON edges.from = radius.node
    WHERE radius.depth < 4
  )
  SELECT node, depth
  FROM radius
`
```

Recursive traversal belongs in SQLite first. Host ops can become sugar after the SQL form proves the row shape.

### Debug Refs

```sprf
sql`
  SELECT input.__cursor_idx, refs.id, refs.file_id, refs.lo, refs.hi
  FROM input
  JOIN _refs AS refs
    ON refs.file_id = input.file
   AND refs.lo <= input.byte
   AND refs.hi > input.byte
`
```

Core table exposure can wait until `_refs` column shape is stable enough for LSP completions.

## LSP Plan

`sql`` has a CST SQL DSL provider for body-local editor features.

Implemented body-local behavior:

```text
SQLite syntax highlighting
completion for SQLite keywords
completion for input.__cursor_idx and input.value
hover for input, __cursor_idx, value, SQL keywords, and host holes
```

Remaining LSP behavior:

```text
completion for current input columns
completion for rule names in current file
completion for rule columns
completion for exposed core tables
diagnostics for unknown rule/table
diagnostics for unknown column
diagnostics for rejected identifier interpolation
```

Later:

```text
EXPLAIN QUERY PLAN as hint diagnostics
go-to rule definition from SQL table name
hover showing physical table rewrite
completion for imported rule namespaces
```

## Optimization

First implementation delegates optimization to SQLite.

Store/lowering responsibilities:

```text
declare rule columns
index rule columns
use temp input relation
batch inserts into input
prepare query once per SQL op shape when possible
surface prepare/query errors as diagnostics
```

Avoid per-cursor SQL. Avoid custom join planning in Rust until SQLite proves insufficient.

## Tests

Target examples first:

```text
sql-join.target.sprf
sql-missing-antijoin.target.sprf
sql-blast-radius.target.sprf
```

Implementation tests later:

```text
sql`...` with input preserves __cursor_idx
${OP} rewrites to input.OP
unknown rule table emits diagnostic
unknown column emits diagnostic
identifier interpolation is rejected
anti-join emits only missing left rows
join emits one cursor per matching right row
OpenAPI op without frontend hook emits one warning
matching hook clears warning after invalidation/recompute exists
```

## Assumptions

SQL-first is the default path. Host ops like `join`, `missing`, `select`, and `where_eq` can become sugar over generated SQL later.

`sql`` is batch-local by default. Whole-generation queries require an explicit future barrier.

SQLite is the semantic model and first execution engine. Trait-backed store boundaries stay, but the v4 query contract is SQLite-shaped.

Current executable docs should distinguish `lsp_warn` from target dotted forms like `lsp.warn`.
