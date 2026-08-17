---
created: 2026-08-16
updated: 2026-08-16
type: feature
status: done
priority: low
epic: extract-port-closeout
labels:
- pkg:extract
- size:small
closed: 2026-08-16
closed_by: extract-driver
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

## Comments

### 2026-08-17T02:59:40Z · @extract-driver

VERIFIED GREEN at origin/main a4045153e by extract-driver in a clean worktree: build rc=0, cargo test --features cli all ok. Landed as 07832acf3 (wire types + schema), 9a65fbb92 (the ts unresolved walker, four rules over the same oxc parse), dab50a2df (tests/20_unresolved.rs grades the marker rows and their negatives). Present: Unresolved at src/types.rs:500, UnresolvedReason at :509, SCHEMA line src/schema.rs:37 with the closed vocabulary pinned at :139 (dynamic-import | computed-member-call | spread-call-args). TS/JS-only scope held, matching v5's v1 scope. Closing.
