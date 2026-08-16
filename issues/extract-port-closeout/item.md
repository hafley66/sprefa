---
created: 2026-08-16
updated: 2026-08-16
type: epic
status: open
priority: high
labels:
- pkg:extract
---

## Description

Goal, in the user's words: finish the v5-to-v6 extract port done-done so nobody
opens dl5 again.

## Census: every v5-vs-v6 extract gap, with a receipt

| # | gap | v5 receipt | v6 receipt | issue | verdict |
|---|---|---|---|---|---|
| 1 | `.dl6` phase-2 arms exist and are never dispatched | n/a (v6-native) | `lang/dl6/_0_source.rs:425,449` vs `project.rs:449-478` | @extract-dl6-resolve-unwired | dispatch |
| 2 | SCIP indexer roster short 3 langs | `src/scip_setup.rs:66-72,80-99` | `scip_ensure.rs:65-88`, gap named at `:35-40` | @extract-scip-indexer-roster | dispatch |
| 3 | docs facet unported, all langs | `typegraph/mod.rs:161-177`, `family/mod.rs:507` | zero `DocFact` in `types.rs` | @extract-docs-facet-shape, @extract-docs-facet-lang-arms | dispatch |
| 4 | df aux fields/lits | `family/mod.rs:455-491` | `types.rs:540-543` | @extract-df-aux-fields-lits | dispatch |
| 5 | df aux loops/nests/allocates | `family/mod.rs:455-491` | `types.rs:540-543` | @extract-df-aux-loops-nests | dispatch |
| 6 | kotlin type plane (candidates + `Resolve<TypeF>`) | `typegraph/kotlin.rs` | `lang/kotlin.rs:27-31`, `project.rs:463-478` | @extract-kotlin-type-plane | dispatch |
| 7 | no python arm at all | `typegraph/python.rs`, `modgraph/python.rs` | `lang/mod.rs:40-51` | @extract-python-arm | dispatch |
| 8 | module plane is TS-only | `family/mod.rs:397-408`, `modgraph/*` | `deps.rs:1-2`, specifiers only in `lang/ts.rs` | @extract-module-plane-non-ts | dispatch (shape depends on #14) |
| 9 | runtime-computed edge markers | `family/mod.rs:552-570` | none | @extract-unresolved-markers | dispatch |
| 10 | markdown doc_node / doc_ref | `family/mod.rs:493-501` | `lang/markdown/_0_source.rs:1-7` (cst only) | @extract-doc-node-markdown | dispatch |
| 11 | content-keyed cache + parallel dispatch | n/a | `types.rs:50-53,1838-1839`, `dispatch.rs:1-16` | @extract-blob-cache-parallel | dispatch |
| 12 | `Resolve<F>` default body is `todo!()` | n/a | `types.rs:1105-1109` | @extract-resolve-todo-default | needs-chris |
| 13 | `DfEdgeKind::Flow` union commented out | n/a | `types.rs:605-612` | @extract-df-flow-union | needs-chris |
| 14 | `ModuleF` collapsed, flagged for human review | `family/mod.rs:397-408` | `types.rs:629-645` | @extract-modulef-collapse | needs-chris |
| 15 | `scip_occurrence` / `scip_binding` outside the v5-vocab set | `src/rels/scip.rs:41-50,77-88` | `schema.rs:160-173` | @extract-scip-vocab-occurrence-binding | needs-chris (occurrence = doc close) |
| 16 | rust type graph as a drawn board | n/a | `types.rs:226-253` `TypeEdgeKind` | @rust-typegraph-d2 | dispatch |

## Closed with no code owed

| v5 thing | why v6 already answers it | receipt |
|---|---|---|
| `comment_node` (`family/mod.rs:532`) | the CstF plane emits `comment` nodes for every grammar; probed on a `.ts` file, 2 comment nodes for 2 comments | `src/lang/astgrep.rs:167`, probe under `extract probe.ts` |
| `template_parts` (`family/mod.rs:549`) | CstF emits `template_string` / `template_substitution` / `string_fragment` as nodes with child edges; v5's `idx`/`kind` row is a join over those children | same probe |
| `call_kind` (`family/mod.rs:448`) | engine-side, not extract: computed over `call_site` in the engine | `src/engine/family/call_kind.rs:2-6` |
| `string` / `ref` / `node` / `child` (`family/mod.rs:585,599`) | engine meta-table views and the CstF plane respectively | `family/mod.rs:578-599` |
| "sprefa-extract has no markdown extractor" (CLAUDE.md open row) | STALE. `MarkdownSource` is in the roster and `source_for(".md")` returns it | `src/lang/mod.rs:46` |

## Baseline, measured 2026-08-16 at 988e2b5

```
cd v6/sprefa-extract && cargo build --all-targets --features cli   # rc=0
cd v6/sprefa-extract && cargo test --features cli                  # rc=0, all legs pass
```
