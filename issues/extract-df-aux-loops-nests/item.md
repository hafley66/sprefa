---
created: 2026-08-16
updated: 2026-08-16
type: feature
status: open
priority: normal
epic: extract-port-closeout
labels:
- pkg:extract
- size:med
blocked_by: ['@extract-df-aux-fields-lits']
---

# df aux: loop_over, allocates and nest

## Description

## Description

The graph-shaped half of v5's df aux: `loop_over`, `allocates`, `nest`. Unlike
fields/lits these are not labels — `nest(call_id, loop_id, depth, collection)`
is what turns `call_edge` into symbolic Big-O.

## Receipts

| fact | receipt |
|---|---|
| v5 rels + semantics | `src/engine/family/mod.rs:455-491` (`loop_over` = each loop's span + variable; `allocates` = fns whose body builds a collection; `nest` = each call's enclosing loop nest, depth + collection) |
| v6 aux | `v6/sprefa-extract/src/types.rs:540-543` — none of the three exist |
| v6 has the node kind already | `src/types.rs:560` `DfNodeKind::Loop` |
| deferral note | `lang/ts.rs:1697` ("the loop FACT is deferred aux"), `src/types.rs:1836` |

Blocked by @extract-df-aux-fields-lits: same aux struct, same four lang files.

## Fix shape

`DfLoop`, `DfAllocates`, `DfNest` on `DfFAux`; three `FlatFact` arms + three
SCHEMA lines; per-lang emission at the loop/call walks that already run.
`nest` depth is computed from the walk's own loop stack, never a second traversal.

## Gate

```bash
cd v6/sprefa-extract
cargo build --all-targets --features cli
cargo test --features cli
cargo test --features cli --test 2_df_aux_cli
cargo test --features cli --test golden_parity
```
