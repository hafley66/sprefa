---
created: 2026-08-16
updated: 2026-08-16
type: feature
status: done
priority: normal
epic: extract-port-closeout
labels:
- pkg:extract
- size:med
blocked_by: ['@extract-docs-facet-shape']
closed: 2026-08-16
closed_by: extract-driver
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

## Comments

### 2026-08-17T02:59:40Z · @extract-driver

VERIFIED GREEN at origin/main a4045153e by extract-driver in a clean worktree: build rc=0, cargo test --features cli all ok. Landed as 6dffe5794 (ts doc walker), e0963329e (go), 4c14ca3e6 (kotlin), 02a205c00 (tests/19_docs_lang_arms.rs, ts 8 + go 6 oracle rows, kotlin self-graded), 2a9759272 (comment rewrap). DocFact emission sites now: src/lang/ts.rs:208, src/lang/go.rs:235, src/lang/kotlin.rs:321, alongside the rust arm at src/lang/rust.rs:187. Oracle census over tests/fixtures/*/*.v5.jsonl carries 19 doc rows. Card deliverable complete; closing.
