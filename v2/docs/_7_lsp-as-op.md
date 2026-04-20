# LSP as op [tentative]

Source: chat 2026-04-18. Author confirmed direction; concrete op
shapes and capability list are sketches, not locked.

## Thesis

LSP behavior is sprf-authored, not Rust-compiled. The server is a
thin dispatcher: an LSP event arrives, the dispatcher looks up
registered ops keyed on `(capability, target_node_kind)`, drives
their pipeline, and serializes the resulting cursor as the LSP
response.

The substrate already exists in v2:

- Cursors carry `content`, `byte_range`, `fs`/`repo`/`rev`, slots,
  and term bindings — every LSP response shape (text + range +
  structured payload) maps onto a cursor.
- Ops already own diagnostics, hover render, and projections. The
  generalization is "op runs at LSP-event time" instead of "op runs
  at extraction time." Same trait family, new dispatch trigger.

## Op shape

The op-name slot picks the LSP capability. The bracket slot picks
the target node kind in some grammar. The brace body is an arbitrary
sprf pipeline producing the LSP payload as cursor content.

```
lsp[hover](walker.dict_value) {
    > render(`bound to: ${target.path}\nrows: ${count}`)
}

lsp[diag](ast.metavar where ${X} unbound) {
    > emit(severity: error, msg: `unknown metavar ${X}`)
}

lsp[complete](walker.field_position) {
    > suggest(&{cursor.fs.parent}/*.json | json(.keys))
}
```

Capability slot vocabulary (tentative): `hover`, `diag`, `complete`,
`def`, `ref`, `codeaction`, `semantic_token`, `inlay`, `rename`,
`format`. One op per (capability, node kind); registry handles
overload by node kind.

## Why it works

- **Cursor as response shape.** Hover text = `cursor.content`.
  Diagnostic range = `cursor.byte_range`. Completion items =
  multi-cursor stream. No new types, just existing cursor consumers.
- **Diagnostics already op-owned.** v2 invariant ("ops own their
  diagnostics") generalizes cleanly: extraction-time diags and
  LSP-time diags are the same machinery, different trigger.
- **Per-op grammars give per-node dispatch for free.** The host knows
  every op's grammar; the LSP server can compute "what node kind is
  the cursor over" by walking the op's inner tree at the cursor
  position. Lookup table is `(grammar_id, node_kind) → registered ops`.

## Programmability ceiling

The unga-bunga question: is this enough for "general programming
inside the body"? The pieces:

- **Composition**: pipe.
- **Variables**: terms (`${X}`).
- **Interpolation**: `${...}` in strings, `&{...}` for structural
  addresses.
- **Branching**: ternary on Ref Tri (planned host node, see
  `_0_term-language.md`).
- **Iteration**: implicit, every op is a stream of cursors.
- **Arithmetic / string ops / collection ops**: ship as builtin ops
  (`str[concat]`, `num[add]`, `seq[map]`).

The grammar surface stays tiny. The language grows by op count, not
by syntax. Every new builtin op = "one more verb in the dispatch
table," no host parser change.

## Dispatch flow

```
LSP event (text doc + position + capability)
    │
    ▼
┌────────────────────────────────────────┐
│ Server: parse host + injected sub-trees│
│         (already cached from extraction)│
└────────────────────────────────────────┘
    │
    ▼
┌────────────────────────────────────────┐
│ Find smallest node enclosing position  │
│         → (grammar_id, node_kind)      │
└────────────────────────────────────────┘
    │
    ▼
┌────────────────────────────────────────┐
│ Registry lookup:                       │
│   (capability, grammar_id, node_kind)  │
│   → Vec<Arc<dyn Operator>>             │
└────────────────────────────────────────┘
    │
    ▼
┌────────────────────────────────────────┐
│ Drive each op's pipe with a synthetic  │
│ root cursor pointing at the node       │
└────────────────────────────────────────┘
    │
    ▼
┌────────────────────────────────────────┐
│ Serialize emitted cursors as LSP        │
│ response shape (Hover / Diagnostic /   │
│ CompletionItem / etc)                  │
└────────────────────────────────────────┘
```

## Layer separation

Three layers, kept mentally distinct:

1. **Parse layer** — tree-sitter, host + per-op grammars. Designed,
   not coded.
2. **Runtime layer** — cursors + ops + pipe. Exists in v2, mostly
   survives into v3.
3. **Programmability layer** — `lsp[...]`, `if[...]`, `str[...]`,
   redirection (see `_8_string-redirection.md`). The frontier. Each
   builtin op is a small feature once layers 1 and 2 land.

The pidgeonholing trap to avoid: hardcoding "this op produces text,
that op produces ranges, the other op produces nodes." Cursor + slot
declarations dissolve that. Every op emits cursors with whatever
slots it declares; extraction, LSP, and render all consume the same
shape. The bash-pipe-into-anything property falls out of "everything
is a cursor."

## Open

- Capability list is a sketch; lock the vocabulary against the LSP
  spec's actual method names (`textDocument/hover`,
  `textDocument/completion`, etc).
- Registration syntax: do `lsp[...]` ops live in the user's `.sprf`
  file (per-project LSP behavior), in a global config, or both?
- Performance: LSP demands sub-100ms responses for hover/complete.
  The runtime needs an LSP-fast path that bypasses extraction setup.
- Re-entrancy: an `lsp[complete]` op runs sprf pipelines that may
  themselves trigger LSP events. Bound recursion or punt entirely.
