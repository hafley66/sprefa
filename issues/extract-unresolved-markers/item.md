---
created: 2026-08-16
updated: 2026-08-16
type: feature
status: open
priority: low
epic: extract-port-closeout
labels:
- pkg:extract
- size:small
---

# runtime-computed edge markers (the unresolved rel)

## Description

## Description

v5 emits a marker row wherever an edge exists but its target is computed at
runtime. v6 emits nothing, so a dynamic `import(expr)`, an `obj[key]()` call and
a `f(...args)` spread are each silently dropped by the walks that already see
them.

## Receipts

| fact | receipt |
|---|---|
| v5 rel + closed vocabulary | `src/engine/family/mod.rs:552-570` (`unresolved(file, line, reason, detail)`; reasons `dynamic-import`, `computed-member-call`, `spread-call-args`) |
| v5 emitter | `src/graph/typegraph/ts/text.rs`, `src/graph/typegraph/ts/mod.rs` (`UnresolvedRef`) |
| v6 has none | no hit for `UnresolvedRef` or `unresolved` as a record under `v6/sprefa-extract/src` |
| diet deps names the same blind spot from the other side | `v6/sprefa-extract/src/deps.rs:33-37` ("what diet cannot see at all: dynamic `import()` with a computed specifier, `require(...)`, `import x = require(...)`") |

## Fix shape

TS/JS only, matching v5's v1 scope. One `Unresolved { span, reason, detail }` on
`CallFAux`, one `FlatFact` arm, one SCHEMA line
(`record=unresolved  family=call  span={start,end}  reason=<slug>  detail=<string>`),
and emission at the three oxc walk sites that already visit the shape and drop
it: the import walk (`lang/ts.rs:892-1010`), the call-site walk
(`lang/ts.rs:841`), the df arg walk (`lang/ts.rs:1729`). `reason` stays the
closed v5 vocabulary; a fourth reason needs its own issue.

## Gate

```bash
cd v6/sprefa-extract
cargo build --all-targets --features cli
cargo test --features cli
cargo test --features cli --test 7_diet_deps_cli
```
