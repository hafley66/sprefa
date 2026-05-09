# V4 Design Guardrails

## Source Priority

| Source | Use |
| --- | --- |
| `human-goals.md` | current intent |
| `v4/src`, `v4/tests`, `v4/examples` | current executable behavior |
| archive README and old `.sprf` files | historical behavior references and examples |
| chat logs / memories / beads | timestamped evidence, not gospel |

When sources conflict, current user intent and current code/tests win.

## Terminology

Use existing project terms first:

```text
cursor
term
ref
source row
rule row
fact row
missing row
generation
tick
store
pipe
op
```

Use SQL terms when the concept maps 1:1:

| SQL term | Sprf use |
| --- | --- |
| relation | table-shaped row set |
| left row | upstream/source row |
| right row | queried relation row |
| join | require matching right rows |
| anti-join / `NOT EXISTS` | keep left row when no right row exists |
| projection | selected output columns |
| predicate | `WHERE` condition |
| view | derived relation |
| materialized view | persisted derived relation |
| index | lookup structure |
| transaction | generation commit |

Avoid durable new nouns during explanation. If a temporary phrase appears, map it immediately to existing terms and do not reuse it as canonical unless the user adopts it.

## Language Shape

Sprf can become general in the shell sense:

- lots of useful ops
- explicit environment/config/source operators
- side effects through visible ops
- events through `next` / `next?`
- durable rows through rules/facts

The host language should not become a large general-purpose expression system. The core should be relation-producing pipes over cursors. Most power belongs in ops and DSLs.

## SQL Semantics, Trait Implementation

Semantics can be explained in SQLite terms:

```text
rule rows form relations
rule queries lower to joins / projections / grounded filters
dotted rule apply lowers to send/run/write
missing lowers to anti-join / NOT EXISTS
generation commit behaves like a transaction
```

Implementation remains trait-backed:

```rust
trait SprfStore {
    fn declare_relation(...);
    fn insert_batch(...);
    fn select_where(...);
    fn anti_join(...);
    fn commit_generation(...);
}
```

Memory SQLite is acceptable as an implementation path. The trait boundary exists to support at least one SQL driver and avoid baking the language directly into one storage library.

## LSP Boundary

Core emits facts:

```text
diagnostic rows
hover rows
link rows
code-action rows
blast-radius rows
```

The LSP adapter maps those rows to protocol types. Core should not depend on LSP protocol structs.

## Module Boundary

Keep `effect_runtime` generic and boring:

```text
queue
component
pipe
node
generation
event bus
timer / next wake mechanics
batch mechanics
```

Keep debated sprf semantics in core:

```text
cursor
term
ref
SprfStore
rule
fact/query semantics
missing / anti
zero-output failure
row provenance
string interning and norm
```

Core and store are one conceptual unit for now.

## File Naming

Do not use numeric filename prefixes for new examples/docs unless explicitly requested. Reading order belongs in `README.md` or doc lists. If an order marker is unavoidable, use a suffix such as `str-rule_1.sprf`, not `1_str-rule.sprf`. Renaming files to create order is path churn.
