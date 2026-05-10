---
name: sprf-lsp-cst-stack
description: [v4 planning] Tree-sitter incremental parse + rowan red/green CST + injection mechanics. Load when modifying sprefa_parse, designing new sub-grammars, or thinking about CST representation tradeoffs.
---

# CST stack: rowan + tree-sitter + injections

## Rowan (rust-analyzer's CST library)

Two layers, one persistent and one ephemeral.

```
   Green tree (the data)             Red tree (the cursor)
   ───────────────────               ───────────────────
   GreenNode { kind, children }      SyntaxNode<'a> {
   GreenToken { kind, text }            green: &GreenNode,
                                        parent: Option<&SyntaxNode>,
   immutable, structurally shared,      offset: TextSize,
   refcounted, hash-consed              index_in_parent: u32,
   no parent pointers                }
                                     ephemeral, walkable both ways,
                                     created on demand from green
```

Edits return a new green tree that shares 99% of nodes with the prior one (im::Vector style). Red layer is wrappers you make and throw away. So "the AST" is two things: a persistent value you keep, and a view you summon.

Why care: lossless (every byte recoverable), errors-as-nodes (no bail), fast diff because structural sharing makes "did this subtree change" a pointer compare.

Variants:
- `rowan` — original, ra extracted it
- `cstree` — fork with built-in interner, better for big files
- `biome_rowan` — biome's fork, adds Cow-friendly slot access

v3 does not use rowan today (uses tree-sitter directly via `OpInvocation::node`). Adding a rowan layer is unnecessary unless v3 starts producing its own CST distinct from tree-sitter.

## Tree-sitter incremental

```
   Tree::edit(InputEdit {
       start_byte, old_end_byte, new_end_byte,
       start_position, old_end_position, new_end_position,
   })
   then Parser::parse(src, Some(&old_tree))
```

Reuses unchanged subtrees. ERROR/MISSING nodes appear inline; you traverse as if nothing happened and emit diagnostics off ERROR nodes. ast-grep treats ERROR nodes as non-matches; helix/zed light them red.

Wiring: take LSP `TextDocumentContentChangeEvent`, convert ranges via `line-index` (see sprf-lsp-server-libs), feed to `Tree::edit` then reparse.

## Injections

Tree-sitter `Parser::set_included_ranges(&[Range{...}])` produces a separate `Tree` over the carved bytes with byte offsets preserved against the host buffer. Same pattern as `sprefa_parse::host_parse_with_injections` produces `InjectedTree` per pattern-op call-site whose op has a registered sub-grammar.

```
   host CST          ▓▓▓▓▓▓▓▓▓▓▓▓▓▓▓▓▓▓▓
                          ↑       ↑
                          │       │
                       included ranges carved
                          │       │
                          ▼       ▼
   injected tree     ▓░ regex tree
                          ↑       ↑
                          │       │
                       same byte coords as host
```

helix and zed only use injections for highlight/indent. The two systems that lifted injected DSLs into a *semantic* layer are stack-graphs (declarative graph) and Volar (virtual files + offset map). See sprf-lsp-multi-dsl-patterns.

## In v3

- `v3/crates/sprefa_parse/src/parse.rs:host_parse_with_injections` — the host parse + injection entry point.
- `v3/crates/sprefa_parse/src/ast.rs:InjectedTree` — one per pattern-op call-site with a sub-grammar. `host_node` carries the `paren_slot` ParseSite; `tree` is the injected CST.
- `v3/crates/pipeline/src/_1_op.rs:Op::language()` and `Op::highlights()` declare the sub-grammar (move these to a `DslGrammar` trait per sprf-lsp-lower-traits).

## What to read

- ra `lib/syntax/src` — rowan in real use.
- `apollographql/apollo-rs` `ARCHITECTURE.md` — rowan applied to GraphQL.
- ast-grep `crates/core/src/source` — `Doc` trait that abstracts over tree-sitter.
- helix `helix-core/src/syntax.rs` — minimal injection wiring at industrial scale.
