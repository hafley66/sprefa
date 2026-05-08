# V4 Rule Query Semantics

## Rule Meaning

A rule is a named pipe that derives rows.

```sprf
rule(:name, A?, B?) {
  ... > pattern > ...
}
```

Terms declared in the rule signature are output columns. Captures inside the body populate cursor terms and become row fields when the rule writes.

## Rule Calls

Rule calls should be explicit. Avoid positional guessing.

Preferred target shape:

```sprf
openapi_ops(OP: OP?)
frontend_hooks(OP: OP, REF: REF?)
```

Call arg meanings:

| Arg shape | SQL meaning |
| --- | --- |
| omitted | no predicate and no projection for that column |
| `OP: OP` | predicate: column `OP` equals current term `OP` |
| `OP: OP?` | projection: bind column `OP` into term `OP` |
| `OP: null` | predicate: column `OP IS NULL` |
| `OP: OTHER?` | projection: bind column `OP` into term `OTHER` |

This gives LSP a clear signature surface: known column names, expected binding modes, and missing arguments.

## Join

Normal rule calls are joins/projections against rule relations.

```sprf
openapi_ops(OP: OP?)
> frontend_hooks(OP: OP, REF: REF?)
```

SQL shape:

```sql
SELECT hooks.REF
FROM left_rows
JOIN frontend_hooks AS hooks
  ON hooks.OP = left_rows.OP;
```

Runtime shape should be batched:

```text
collect distinct OP values from cursor batch
query frontend_hooks where OP in (...)
splice matching rows back onto matching cursors
```

## Missing

`missing(...)` is anti-join / `NOT EXISTS`.

```sprf
openapi_ops(OP: OP?)
> missing(frontend_hooks(OP: OP))
```

SQL shape:

```sql
SELECT left.*
FROM left_rows AS left
WHERE NOT EXISTS (
  SELECT 1
  FROM frontend_hooks AS hooks
  WHERE hooks.OP = left.OP
);
```

Plain meaning:

```text
keep the left row when the right relation has zero matches
```

`missing` is not `neq`. `neq` compares existing values. `missing` handles no right row existing.

## Predicates

Small predicate set first:

```text
eq
neq
is_null
is_not_null
```

Do not rebuild full SQL in host syntax yet. Use SQL as the semantic model and add small ops only when needed.

## Zero Output Failure

Default: zero output is failure.

For whole-pipe debugging, zero rows from a step can be reported as a failure hint.

For useful checks, zero-output failure should usually be modeled through SQL terms:

```text
left relation row exists
right relation should exist
anti-join produces missing row when right relation does not exist
missing row feeds lsp.warn / lsp.error
```

## Example Target

```sprf
rule(:missing_frontend_hooks, OP?) {
  openapi_ops(OP: OP?)
  > missing(frontend_hooks(OP: OP))
  > lsp.warn`missing frontend hook for ${OP}`
}
```

This target proves:

- explicit keyword projection
- batched rule-table lookup
- anti-join / `NOT EXISTS`
- diagnostic fact emission

