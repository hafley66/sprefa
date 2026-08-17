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
blocked_by: ['@extract-dl6-resolve-unwired']
closed: 2026-08-16
commits:
- hash: d295a0768532f98a75b2e943398ef59bb86becfc
  summary: markdown doc_node + doc_ref bridge
---

# markdown doc_node and the doc-to-code doc_ref bridge

## Description

## Description

`MarkdownSource` projects the CST and stops. v5's document family adds the
doc-to-code bridge: a heading whose text matches a declared entity's name.

## Receipts

| fact | receipt |
|---|---|
| v5 rels | `src/engine/family/mod.rs:493-501` `DOC_RELS = ["doc_node", "doc_ref"]` (`doc_node(file, line, kind, name, parent)`, `doc_ref(file, line, sym)`) |
| v6 markdown is cst-only | `v6/sprefa-extract/src/lang/markdown/_0_source.rs:1-7`, `:105-108`; only `CstF` is filled |
| the CLAUDE.md "no markdown extractor" row is STALE | `MarkdownSource` is in the roster at `v6/sprefa-extract/src/lang/mod.rs:46` and `source_for(".md")` returns it |

## Fix shape

`doc_node` is a projection over the CST already emitted (heading / fenced code
block / section, with the enclosing heading as parent), so it can be a TypeF-
plane aux on the markdown arm rather than a new walk. `doc_ref` is a phase-2
join: a heading name matched against the corpus `DefIndex`
(`cx.indexes.def_index`, `src/types.rs:1100-1102`), which means
`impl Resolve<TypeF> for MarkdownSource` and a dispatch arm at
`project.rs:467-478`.

Land @extract-dl6-resolve-unwired first; it adds the roster rail this arm has
to register with.

## Gate

```bash
cd v6/sprefa-extract
cargo build --all-targets --features cli
cargo test --features cli
cargo test --features cli --test 11_markdown
```
