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
---

# docs facet: DocFact/DocTag shape plus the rust arm

## Description

## Description

The docs facet is unported for every language. v5 binds a cleaned doc block and
its structured tags to each declared entity; v6 emits nothing, and no type,
record or wire tag exists for it.

## Receipts

| fact | receipt |
|---|---|
| v5 shape | `src/graph/typegraph/mod.rs:161-166` (`DocFact { sym, line, text, tags }`), `:173-177` (`DocTag { tag, arg, text }`) |
| v5 rels | `src/engine/family/mod.rs:507` `DOC_TEXT_RELS = ["doc_comment", "doc_tag"]` |
| v5 rust walker | `src/graph/typegraph/rust/mod.rs:519` `rust_docs_from`, called at `:36` and `:72` |
| v6 has zero | `grep -c DocFact v6/sprefa-extract/src/types.rs` = 0; no `doc` tag in `src/schema.rs` SCHEMA |
| named deferred | `v6/sprefa-extract/src/lang/rust.rs:23-24`, `lang/go.rs:19`, `lang/kotlin.rs:27-28`, `tests/golden_parity.rs:22-24`, `src/types.rs:1832` |

## Scope of THIS issue

The SHAPE plus the RUST arm only. The ts/go/kotlin arms are @extract-docs-facet-lang-arms.

1. `DocFact` + `DocTag` types in `src/types.rs`, in the TYPE plane section beside
   `ConstValue`; carried on `TypeFAux` (`src/types.rs:328-332`) as `docs: Vec<DocFact>`.
   Owner is the entity's `Span`, never a v5 `sym` string — v6 identity is the span
   (`src/types.rs:30-34`).
2. Two `FlatFact` arms + two `SCHEMA` record lines in `src/schema.rs`:
   `record=doc  family=type  owner={start,end}  text=<string>` and
   `record=doc_tag  family=type  owner={start,end}  tag=<string>  arg=<string>  text=<string>`.
   Follow the `const` record's existing shape (`schema.rs:32`).
3. Port `rust_docs_from` (`src/graph/typegraph/rust/mod.rs:519`) into
   `src/lang/rust.rs`, emitting into `TypeFAux.docs`. Keep v5's cleaning and tag
   split byte-identical; `# Heading` becomes `tag=section` per
   `src/graph/typegraph/mod.rs:168-171`.
4. Flip the docs row in the status table at `src/types.rs:1831` for rust.
5. `tests/golden_parity.rs:22-24` lists `doc` under DEFERRED v5-only (reported,
   not asserted). Move rust's doc facet to ASSERTED and regenerate the rust
   oracle baseline if it lacks doc rows; if the captured oracle cannot be
   regenerated, keep it reported and say so in a comment.

## Gate

```bash
cd v6/sprefa-extract
cargo build --all-targets --features cli
cargo test --features cli
cargo test --features cli --test golden_parity
```
