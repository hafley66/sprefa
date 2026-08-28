---
created: 2026-08-16
updated: 2026-08-28
type: feature
status: done
priority: normal
epic: extract-port-closeout
labels:
- pkg:extract
- size:large
closed: 2026-08-28
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

## Comments

### 2026-08-17T02:59:16Z · @extract-driver

PARKED by user word 2026-08-16 (relayed via sprefa-coordinator). Stage state at origin/main a4045153e: commits A (6ec7c0212 skeleton, cst via ast-grep + tree-sitter-python parse), B (f684fc89c TypeF entities + arrow-type sigs), C (11b352dbf CallF defs + sites) ARE LANDED; tests/16_python.rs green (7 entities hand-derived from tests/fixtures/python/sample.py). OUTSTANDING: D (DfF, v5 py_dataflow_from), E (type-edge candidates py_edges_from + both Resolve arms), the docs facet (v5 py_docs_from), the module plane (v5 src/graph/modgraph/python.rs), and the roster wiring. Roster wiring has a TRAP nobody has recorded: tests/4_capability_parity.rs:67 uses ROSTER_FIXTURES row ("astgrep", "tests/fixtures/astgrep/sample.py") -- adding PythonSource to lang/mod.rs sources() steals .py from the cst-only fallback and breaks that row, so the astgrep fixture must move to a non-python grammar in the same commit. Deferral ledger the arm keeps on itself: src/lang/python/_0_source.rs:15-19. Not dispatched; card stays open.

### 2026-08-28T04:28:48Z · @sprefa-fable

LANDED 2026-08-28 (branch feat/extract-python-arm): commits D (DfF, byte-exact vs v5 py_dataflow_from), E (type-edge candidates + Resolve<TypeF> + Resolve<CallF>), docs facet (PEP 257 + Sphinx tags, <module> Ext entity for the module docstring), import specifiers (module plane), roster line, RESOLVE_ARMS row, ROSTER_FIXTURES row (astgrep fixture moved to sample.html), V5_ORACLE_LANGS row. Oracles: tests/fixtures/python/{sample,docs,flow}.v5.jsonl via examples/v5_normalize.rs (.py arm added). Gate: 16_python 6/6, golden_parity 10/10, 19_docs_lang_arms 4/4.

