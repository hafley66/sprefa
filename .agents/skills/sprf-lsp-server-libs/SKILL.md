---
name: sprf-lsp-server-libs
description: [v4 planning] Rust LSP server library landscape (lsp-server, tower-lsp, async-lsp) plus position-math helpers (line-index, lsp-positions). Load when wiring or refactoring v3/crates/server.
---

# LSP server libs + position math

## Wire layer (pick one)

| Crate | Style | State access | Live? | Used by |
|---|---|---|---|---|
| `lsp-server` (rust-analyzer's) | sync stdio JSON-RPC, ~600 LoC, no async, hand-rolled dispatch | direct, you own the loop | yes | rust-analyzer itself |
| `tower-lsp` | async, `&self` handlers, locks for state mutation | `Arc<Mutex<…>>` everywhere | original repo unmaintained | ast-grep-lsp, harper-ls |
| `tower-lsp-server` | community fork of tower-lsp | same as tower-lsp | yes | newer servers |
| `async-lsp` (oxalica) | async, `&mut self` handlers, tower middleware (timing, concurrency, catch-unwind) | direct, no locks | yes | the only one with real middleware composition |

Today v3/crates/server runs on tower-lsp. No symptom forces a switch. async-lsp is the live alternative if you outgrow `Arc<Mutex<…>>`.

## Helpers worth knowing

- `lsp-types` — wire types, used by all of the above
- `lsp-async-trait` — small ergonomic helper for tower-lsp
- `tower-lsp-textdocument` — synchronizes didChange into a buffer, ~200 LoC, often inlined
- `helix-lsp` — extracted from helix as a library-shape *client* (not server)

There is no "Express for LSP" in Rust. Every server is hand-rolled. The high-value architecture (incremental memo + arena CST + per-file boundaries) is too opinionated for a generic crate.

## Position math (the UTF-16 trap)

LSP positions are `{ line: u32, character: u32 }` in **UTF-16 code units**. Source is UTF-8. A 4-byte emoji is 1 grapheme, 2 UTF-16 units, 1 codepoint, 4 UTF-8 bytes. All four numbers are different and you will use the wrong one.

```
   src bytes:   "let x = 🎉;"
                 0   4   8   9   13  14
   utf-8:        l e t   x   =   🎉    ;
                 0 1 2 3 4 5 6 7 8 9 ... 13 14

   utf-16:       l e t _ x _ = _ 🎉🎉 ;
                 0 1 2 3 4 5 6 7 8  9 10
                                  ↑──↑
                                  surrogate pair, 2 code units

   LSP says:     line 0, character 9   ← the ;
   you compute:  byte 13               ← need this
```

Use one of:

- `line-index` — extracted from rust-analyzer, on crates.io. Eats source once, returns newline offsets + UTF-16 widths per line, converts in O(log n).
- `lsp-positions` — from `github/stack-graphs`. Carries `Position { line, column: { utf8, utf16, grapheme } }` so you keep all three at once.

Hand-rolling the conversion is a four-day debugging session every time someone uses an emoji in a comment.

## Decision shape

```
   need bidirectional client+server?     async-lsp
   already on tower-lsp, working fine?   stay
   want zero abstraction, ra-style?      lsp-server
   need middleware (timing, retries)?    async-lsp (only one with this)
   need UTF-16 ↔ byte conversion?        line-index OR lsp-positions; not hand-roll
```
