---
name: sprf-lsp-multi-dsl-patterns
description: [v4 planning] Patterns for lifting injected sub-DSLs into semantic LSP features — Volar virtual files, stack-graphs, helix injection-as-highlight. Load when adding semantic features (hover/complete/jump) to a sub-grammar.
---

# Multi-DSL semantic features

Tree-sitter native injections give you highlight/indent/locals only. Lifting injected DSLs into hover/complete/jump-to-definition needs more.

## The three real patterns

| Pattern | Project | What it does | Limits |
|---|---|---|---|
| Highlight-only | helix, zed | `injections.scm` carves ranges; per-language highlight queries run | no semantics, no LSP on injected layer |
| Virtual file + offset map | Volar (Vue), Astro, Svelte, Angular | synthesize a TS file from the SFC; route LSP requests to tsserver via offset map | TS-specific framework; pattern itself is general |
| Declarative graph (TSG) | stack-graphs (GitHub) | `tree-sitter-graph` DSL builds name-resolution graph from CST patterns | name-res only, no types, no rewriting |

## Volar pattern (the most general)

```
   on disk:                  in memory (virtual):
   ┌──────────────┐          ┌──────────────┐
   │ <script>     │          │ const props  │
   │   const x=1  │ ──map──► │   = 1        │
   │ </script>    │          │ ;import _v   │
   │ <template>   │          │ /* template  │
   │   {{x*2}}    │          │    becomes   │
   │ </template>  │          │    JSX */    │
   └──────────────┘          │ _v(x*2)      │
                             └──────────────┘
                                    │
                                    ▼
                                 tsserver
                             (thinks it's TS)
                                    │
                                    ▼
                          hover at offset N in
                          synthetic file
                                    │
                          unmap N → offset M
                          in real .vue
                                    │
                                    ▼
                              show to client
```

Volar = "offset translation layer + virtual-file FS shim, sitting between LSP client and a host language server."

Cascading source maps when injections nest (template inside script inside SFC). Apply the same in v3: when a `re(...)` body contains `${X}` carveouts pointing at sprefa cursor fields, the offset map carries the host-DSL provenance.

## stack-graphs pattern

TSG (tree-sitter-graph) is a declarative DSL: TS query patterns build graph nodes and edges from CST. Resolution algorithm walks symbol/scope stacks across files.

```
   .tsg file:
     (function_definition name: (identifier) @name) {
        node @name.def
        edge @name.def -> JUMP_TO_SCOPE
        attr (@name.def) symbol = (source-text @name)
     }

   produces a graph; resolution = walk symbol/scope stacks
```

Scales to billions of LOC at GitHub. Cap: name-resolution only; no type inference, no flow analysis, no rewriting.

For sprefa: revisit when DSL count crosses ~6 *and* you need cross-DSL name resolution. Today `pipeline::binding_graph` does this in code.

## helix pattern (highlight-only)

```
   queries/<lang>/injections.scm:
     (call_expression
       function: (identifier) @injection.language
       arguments: (arguments (string (string_content) @injection.content))
       (#eq? @injection.language "sql"))

   carves ranges → separate Tree per language → highlight queries run
```

That's it. No hover, no jump, no complete on the injected layer. helix doesn't try.

## Decision tree for sprefa

```
   need highlight only on injected DSL?              tree-sitter injections + highlights.scm
                                                     (already supported by Op::highlights)

   need hover/complete/diag inside injected body?    Volar virtual-file pattern
                                                     synthesize a doc per body, route requests,
                                                     translate offsets back

   need cross-DSL name resolution?                   stack-graphs (TSG) OR keep current
                                                     binding_graph and hand-roll
                                                     (defer until 6+ DSLs)

   need to rewrite/format injected bodies?           write your own; nobody has a generic answer
```

## In v3 today

- Highlight wiring is in `pipeline::_1_op::Op::{language, highlights}` and `sprefa_parse::host_parse_with_injections` (see sprf-lsp-cst-stack).
- No virtual-file / Volar layer yet.
- No stack-graphs integration; `pipeline::binding_graph` is the in-tree analog.
