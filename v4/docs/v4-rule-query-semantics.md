# V4 Rule Query Semantics

## Rule Meaning

A rule is a named relation with an optional body that derives rows.

```sprf
rule(:name, A?, B?) {
  ... > pattern > ...
}
```

The rule signature declares the relation schema. The body, when present, is a producer for that relation. Empty-body rules are relation subjects: declare a table, accept explicit writes, and replay/query rows later.

```sprf
rule(:frontend_hooks, OP!, FILE?, REF?);
```

This means:

| Surface | Meaning |
| --- | --- |
| `rule(:frontend_hooks, OP!, FILE?, REF?);` | declare relation/table |
| `... > rule(:frontend_hooks, OP, FILE)` | write/next into relation |
| `... > frontend_hooks(OP, FILE?)` | read/query relation rows |
| `... > frontend_hooks?(OP, FILE)` | predicate/filter with `EXISTS` |

The old `fact` idea maps to an empty-body `rule`: declaration plus imperative row signal/write plus replay/query of the table.

## Declaration Sigils

Rule column sigils are schema metadata.

| Sigil | Meaning |
| --- | --- |
| `NAME` | required column |
| `NAME?` | nullable/projectable column |
| `NAME!` | key/unique/upsert identity column |

`!` is target metadata first. SQLite-backed execution should lower it to a uniqueness/upsert key once store schemas carry richer column metadata.

## Rule Calls

Rule calls use Python-style positional and keyword binding against declared columns.

```sprf
frontend_hooks(OP, FILE?, REF: REF?)
```

Current executable syntax applies relation names directly. Target syntax uses dotted apply for relation calls:

```sprf
frontend_hooks.(OP, FILE?, REF: REF?)
frontend_hooks?.(OP, FILE)
frontend_hooks.()
frontend_hooks?.()
```

Under dotted apply, bare `frontend_hooks` is the relation symbol and `frontend_hooks.(...)` applies it to the current cursor batch. The dot is pronounced "apply". Lowering is the same as the direct form. Parser support needs one host grammar change: allow an immediate `.` apply marker between the op name or predicate suffix and the slot list.

| Direct current form | Dotted target form | Meaning |
| --- | --- | --- |
| `frontend_hooks(OP, FILE?)` | `frontend_hooks.(OP, FILE?)` | read/query relation |
| `frontend_hooks?(OP, FILE)` | `frontend_hooks?.(OP, FILE)` | predicate/filter relation |
| `frontend_hooks()` | `frontend_hooks.()` | replay all rows for each input cursor |
| `frontend_hooks?()` | `frontend_hooks?.()` | table-nonempty predicate |

Rules:

| Rule | Example |
| --- | --- |
| positional args bind declared columns by order | `frontend_hooks(OP, FILE?)` |
| kwargs bind declared columns by name | `frontend_hooks(OP: OP, FILE: FILE?)` |
| once a kwarg appears, later positional args are rejected | `frontend_hooks(OP: OP, FILE?)` is invalid |
| duplicate column assignment is rejected | `frontend_hooks(OP, OP: OTHER)` is invalid |
| unknown kwarg column is rejected | `frontend_hooks(NOPE: X)` is invalid |
| positional overflow is rejected | more positional args than declared columns is invalid |

Positional args are authoring sugar. Lowering should normalize every call into named column assignments before producing SQL or a store write.

```text
rule(:frontend_hooks, OP!, FILE?, REF?)

frontend_hooks(OP, FILE?)
=> OP: OP, FILE: FILE?

frontend_hooks(OP, REF: REF?)
=> OP: OP, REF: REF?
```

## Call Arg Meanings

```sprf
frontend_hooks(OP: OP, FILE: FILE?, REF: :source)
```

| Arg shape | SQL meaning |
| --- | --- |
| omitted | no predicate and no projection for that column |
| `OP: OP` | predicate: column `OP` equals current term `OP` |
| `OP: OP?` | projection: bind column `OP` into term `OP` |
| `OP: null` | predicate: column `OP IS NULL` |
| `OP: OTHER?` | projection: bind column `OP` into term `OTHER` |
| `OP: :literal` | predicate/write literal value |
| `OP: &.value` | predicate/write cursor value |

## Write Calls

Sink-position `rule(:name, ...)` writes only the assigned columns.

```sprf
rule(:frontend_hooks, OP!, FILE?, REF?);

... > rule(:frontend_hooks, OP, FILE)
... > rule(:frontend_hooks, OP: OP, FILE: FILE)
... > rule(:frontend_hooks, OP: &.value, REF: REF)
```

Lowering shape:

```text
rule(:frontend_hooks, OP, FILE)
=> write OP = input.OP, FILE = input.FILE

rule(:frontend_hooks, OP: &.value, REF: REF)
=> write OP = input.value, REF = input.REF
```

Sink-position writes should not copy the whole cursor term bag. Cursor terms only enter a row when assigned by position or kwarg.

## Read And Predicate Calls

`rule_name(...)` is row-producing. It queries the rule table and emits one output cursor per matching row at that point in the pipe.

```sprf
frontend_hooks(OP, FILE?)
frontend_hooks.(OP, FILE?)
```

Plain meaning:

```text
for each input cursor:
  find rows where frontend_hooks.OP = input.OP
  emit one output cursor per matching row
  bind output FILE from the matched row
```

`rule_name?(...)` is predicate form. It filters the current input batch with `EXISTS` semantics and projects no output columns. Use this for fully bound checks.

```sprf
frontend_hooks?(OP, FILE)
frontend_hooks?.(OP, FILE)
```

Plain meaning:

```text
pass the input cursor through once when a matching row exists
drop the input cursor when no matching row exists
```

`frontend_hooks?()` / `frontend_hooks?.()` with no args is a table-nonempty gate:

```text
pass each input cursor once if frontend_hooks has at least one row
drop each input cursor if frontend_hooks has zero rows
```

This is still a relation predicate. If `frontend_hooks` was declared as an empty-body rule, the predicate checks rows that were written into the table.

## Relation Read vs Body Invocation

Direct `rule_name(...)` and `rule_name?(...)` calls target the rule relation/table.

```sprf
frontend_hooks(OP, FILE?)   # relation read
frontend_hooks?(OP, FILE)   # relation predicate

frontend_hooks.(OP, FILE?)  # dotted relation read target
frontend_hooks?.(OP, FILE)  # dotted relation predicate target
```

A rule with a body also writes to its relation. Reading the rule name reads the materialized relation rows. Inline body invocation for a caller cursor is a separate future feature.

Future parametric body invocation can be specified separately. Until then, "call" in this document means relation call.

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
