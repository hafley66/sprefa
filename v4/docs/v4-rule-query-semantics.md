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
rule(:frontend_hooks, OP?, FILE?, REF?);
```

This means:

| Surface | Meaning |
| --- | --- |
| `rule(:frontend_hooks, OP?, FILE?, REF?);` | declare relation/table |
| `... > rule(:frontend_hooks, OP, FILE)` | write/next into relation |
| `... > frontend_hooks(OP, FILE)` | apply/send grounded values into relation |
| `... > frontend_hooks?(FILE?, OP)` | query relation rows |
| `... > frontend_hooks?(OP, FILE)` | grounded query, predicate-like pass/drop |

The old `fact` idea maps to an empty-body `rule`: declaration plus imperative row signal/write plus replay/query of the table.

## Dotted Rule Atoms

Rule atoms may use dot-separated identifier segments for lightweight
namespacing:

```sprf
rule(:docs.runtime_node, NAME?, FILE?);
docs.runtime_node?(NAME?, FILE?)
```

Atom spelling:

```text
:[A-Za-z_][A-Za-z0-9_]*(\.[A-Za-z_][A-Za-z0-9_]*)*
```

Termination is lexical. The atom ends at whitespace, comma, `)`, `}`,
`;`, or another character outside identifier/dot segments. A trailing
dot, double dot, slash, dash, or nested colon is rejected by the host
grammar.

The colon is syntax only. Lowering stores the atom value without the
colon, so `:docs.runtime_node` becomes relation name `docs.runtime_node`.
Dots are namespace punctuation in `.sprf`; SQL lowering must still quote
or rewrite physical table names because SQLite treats dots as
schema/table separators.

## Declaration Sigils

Rule column sigils are schema metadata.

| Sigil | Meaning |
| --- | --- |
| `NAME` | required column |
| `NAME?` | nullable/projectable column |
| `NAME!` | key/unique/upsert identity column |

Declaration sigils are not call direction. `!` is target metadata first.
SQLite-backed execution should lower it to a uniqueness/upsert key once
store schemas carry richer column metadata.

## Locked Rule Calls

This section is locked design intent. Implementation may temporarily lag
behind it, but new tests and code should converge here.

Markers:

| Marker | Meaning |
| --- | --- |
| `TERM?` | hole / setter / output projection. This step writes the term. |
| `TERM` | grounded read / constraint. This step reads the term. |
| `rule_name?(...)` | query or replay materialized relation rows. |
| `rule_name(...)` | apply/send/run. Args must be grounded. |
| `rule_name!(...)` | reserved apply policy override. |

`rule_name.(...)` is retired for declared rules. The dot remains available
inside operator names such as `render.markdown`.

Rule calls use Python-style positional and keyword binding against
declared columns. Positional args bind declared columns by order; kwargs
bind declared columns by name. A bare same-name `TERM` or `TERM?`
also binds the declared column with that name, even after kwargs.

```sprf
frontend_hooks?(FILE?, OP)
frontend_hooks?(FILE: FILE?, OP: OP)
```

Query examples:

| Surface | Meaning |
| --- | --- |
| `frontend_hooks?(FILE?, OP)` | query rows where row `OP = cursor.OP`, project row `FILE` into cursor `FILE` |
| `frontend_hooks?(OP, FILE)` | query rows where both match; emits distinct pass-through output cursors |
| `frontend_hooks?(FILE?, OP: OP)` | same as positional, normalized to column assignments |

Apply examples:

| Surface | Meaning |
| --- | --- |
| `frontend_hooks(OP, FILE)` | apply/send/run with grounded `OP` and `FILE` |
| `frontend_hooks!(OP, FILE)` | reserved for future apply policy override |
| `frontend_hooks(FILE?, OP)` | invalid: apply cannot accept holes |

Rules:

| Rule | Example |
| --- | --- |
| positional args bind declared columns by order | `frontend_hooks?(OP, FILE?)` |
| kwargs bind declared columns by name | `frontend_hooks?(OP: OP, FILE: FILE?)` |
| same-name shorthand can follow kwargs | `rule_a?(X?, Y, OUT_A: OUT_A?, OUT_B?)` |
| once a kwarg appears, later non-shorthand positional args are rejected | `frontend_hooks(OP: OP, :literal)` is invalid |
| duplicate column assignment is rejected | `frontend_hooks(OP, OP: OTHER)` is invalid |
| unknown kwarg column is rejected | `frontend_hooks(NOPE: X)` is invalid |
| positional overflow is rejected | more positional args than declared columns is invalid |
| apply args must be grounded | `frontend_hooks(OP, FILE)` |
| apply args cannot be holes | `frontend_hooks(OP, FILE?)` is invalid |

Positional args are authoring sugar. Lowering should normalize every call into named column assignments before producing SQL or a store write.

```text
rule(:frontend_hooks, OP, FILE, REF)

frontend_hooks?(FILE?, OP)
=> FILE: FILE?, OP: OP

frontend_hooks?(FILE?, REF: REF)
=> FILE: FILE?, REF: REF

rule_a?(X?, Y, OUT_A: OUT_A?, OUT_B?)
=> X: X?, Y: Y, OUT_A: OUT_A?, OUT_B: OUT_B?
```

## Call Arg Meanings

```sprf
frontend_hooks(OP: OP, FILE: FILE?, REF: :source)
```

| Arg shape | Query meaning |
| --- | --- |
| omitted | no predicate and no projection for that column |
| `OP: OP` | constraint: column `OP` equals current term `OP` |
| `OP: OP?` | projection: bind column `OP` into term `OP` |
| `OP: null` | constraint: column `OP IS NULL` |
| `OP: OTHER?` | projection: bind column `OP` into term `OTHER` |
| `OP: :literal` | predicate/write literal value |
| `OP: &.value` | predicate/write cursor value |

## Write Calls

Sink-position `rule(:name, ...)` writes only the assigned columns.

```sprf
rule(:frontend_hooks, OP?, FILE?, REF?);

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

The executable implementation keeps legacy `... > rule(:name)` as a whole-cursor write when no assignments are given. Assigned writes are projected rows.

## Query Calls

`rule_name?(...)` is row-producing. It queries the rule table and emits one output cursor per matching row at that point in the pipe.

```sprf
frontend_hooks?(FILE?, OP)
```

Plain meaning:

```text
for each input cursor:
  find rows where frontend_hooks.OP = input.OP
  bind output FILE from matching rows
  emit distinct output cursors by cursor content hash
```

Grounded query is predicate-like:

```sprf
frontend_hooks?(OP, FILE)
```

Plain meaning:

```text
find rows where both columns match the input cursor
emit the distinct pass-through output cursor set
zero rows means failure/drop
```

## Apply Send Run

`rule_name(...)` is apply/send/run. All args must be grounded. Holes are
illegal at this boundary.

For an empty-body rule:

```text
rule(:r, X?, Y?);

r(X, Y)
  write/send grounded X and Y into relation r
  pass cursor through
```

For a bodied rule:

```text
rule(:r, X?, Y?) { ... }

r(X, Y)
  run/apply the body with grounded args
  cache read allowed

r!(X, Y)
  reserved apply policy override
```

Future `!` is reserved for apply-time cache/storage policy. It should not
change query, subscription, or projection semantics.

## Runtime State

Current executable runtime state:

```text
RuleInvokeComponent cache key =
  rule name
  input cursor content hash
  call-site assignments

SqlQueryComponent cache key =
  SQL text
  upstream cursor batch content hashes
  referenced relation row identities
```

Cache hits replay output cursors without rerunning SQLite. When a referenced table gains rows, the row identity portion of the key changes, so the next relation read computes a new result while older cache entries remain available.

Future live subscription state:

```text
query mount =
  source op span / pipe position
  normalized relation call or sql body
  upstream cursor batch identity
  referenced relation tables / row keys
  last output cursor ids
```

That state should be a sprf-core layer over the effect runtime. The effect runtime already has `EventBus`, `Memoize`, `Query`, dirty events, and queue cascade deletion. Sprf-core should own rule-table dependencies, SQL/table invalidation, and LSP/query mount identity because those concepts depend on sprf relations and cursors.

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
