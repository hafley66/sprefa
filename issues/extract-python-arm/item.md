---
created: 2026-08-16
updated: 2026-08-16
type: feature
status: open
priority: normal
epic: extract-port-closeout
labels:
- pkg:extract
- size:large
---

# python front-end arm: the whole lang, mirroring go

## Description

## Description

v5 carries a full python front-end on both planes; v6's roster has no
`PythonSource`, so a `.py` file falls to the cst-only ast-grep fallback and
yields no type, call or df facts.

## Receipts

| fact | receipt |
|---|---|
| v6 roster, no python | `v6/sprefa-extract/src/lang/mod.rs:40-51` |
| a `.py` therefore hits the cst-only fallback | `v6/sprefa-extract/src/lang/mod.rs:7-8`, `src/lang/astgrep.rs:219` |
| v5 type plane | `src/graph/typegraph/python.rs` (entities, sigs, docs at `:638` `py_docs_from`, template parts) |
| v5 module plane | `src/graph/modgraph/python.rs` |
| grammar already in the lock | `cargo tree -p sprefa-extract` shows `tree-sitter-python v0.23.6` as an ast-grep-language transitive — the same argument `lang/kotlin.rs:6-14` used for `tree-sitter-kotlin-sg`, so this needs no new top-level dep ruling |

## Fix shape

Copy the go arm's shape end to end (`src/lang/go.rs`); go is the closest twin
(tree-sitter front-end, raw byte offsets, no line/col bridge):
cst via ast-grep python grammar + one tree-sitter-python parse feeding
type/call/df, then `Resolve<TypeF>` and `Resolve<CallF>`.

Commit split, mirroring go's own header (`lang/go.rs:11-17`):
A skeleton + cst, B type entities + sigs, C call defs + sites, D df nodes +
Direct edges, E type-edge candidates + both Resolve arms.

Roster + dispatch + capability fixture must all land, or the arm is
binary-unreachable: `lang/mod.rs:40`, `project.rs:449` and `:467`,
`tests/4_capability_parity.rs` `ROSTER_FIXTURES`.

## Gate

```bash
cd v6/sprefa-extract
cargo build --all-targets --features cli
cargo test --features cli
cargo test --features cli --test 4_capability_parity
cargo test --features cli --test golden_parity
```
