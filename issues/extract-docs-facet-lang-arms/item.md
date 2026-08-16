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
blocked_by: ['@extract-docs-facet-shape']
---

# docs facet: the ts, go and kotlin walkers

## Description

## Description

The ts, go and kotlin doc walkers, on the shape @extract-docs-facet-shape lands.

## Receipts

| lang | v5 walker |
|---|---|
| ts | `src/graph/typegraph/ts/mod.rs:1036` `ts_docs_from`, called at `:51` |
| go | `src/graph/typegraph/go.rs:681` `walk_go_docs`, called at `:27`, recursion at `:755` |
| kotlin | `src/graph/typegraph/kotlin.rs:1034` `walk_kotlin_docs`, called at `:31`, recursion at `:1076` |

v6 deferral notes: `lang/go.rs:19`, `lang/kotlin.rs:27-28`.

## Fix shape

One walker per lang file, emitting `TypeFAux.docs`, mirroring the rust arm the
shape issue landed. No change to `types.rs`, `wire.rs` or `schema.rs`.

## Gate

```bash
cd v6/sprefa-extract
cargo build --all-targets --features cli
cargo test --features cli
cargo test --features cli --test golden_parity
```
