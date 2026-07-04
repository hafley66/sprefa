---
title: "sprefa: dl, a reactive datalog over your code"
sub_title: facts in, queries out, no bespoke server
author: sprefa
---

What is dl
---

dl is a reactive datalog engine that treats your codebase as a fact database.

<!-- pause -->

* `scan` + `regex`/`ast`/`sg`/`json` extract facts from source, on disk or at a git rev
* rules join and recurse over those facts in plain relational logic
* every rule body is a SQL fixpoint under the hood -- no custom graph walker
* the same engine drives a CLI, an LSP, an MCP server, and a VS Code panel

<!-- pause -->

You write relations. dl figures out how to keep them current.

<!-- end_slide -->

Facts from a scan
---

No hand-written parser. `scan` walks the tree; `sg` matches an ast-grep
pattern and locates it.

```bash +exec_replace +no_background
bat --color always --style=plain --paging=never --language=dl /Users/chrishafley/projects/sprefa/.claude/worktrees/vscode-flow-panel/deck/snippets/facts.dl
```

<!-- pause -->

`CalleeName` is capitalized on purpose: it is an ast-grep metavariable, and
the bound value flows straight into the surrounding rule as a normal variable.

<!-- end_slide -->

The join
---

Two fact tables, one shared column, ordinary relational join -- no special
"call graph" API.

```bash +exec_replace +no_background
bat --color always --style=plain --paging=never --language=dl /Users/chrishafley/projects/sprefa/.claude/worktrees/vscode-flow-panel/deck/snippets/join.dl
```

<!-- pause -->

`call_in_function` reads: a call belongs to the nearest function declaration
at or before it, in the same file. That is the whole join.

<!-- end_slide -->

Recursion: blast radius
---

Transitive closure is a built-in operator, not a hand-rolled traversal.

```bash +exec_replace +no_background
bat --color always --style=plain --paging=never --language=dl /Users/chrishafley/projects/sprefa/.claude/worktrees/vscode-flow-panel/deck/snippets/recursion.dl
```

<!-- pause -->

`closure(type_edge)` walks every field, variant, impl, and generic-bound edge
transitively. Change `Engine::tick`'s shape and this is everything downstream
of it, across Rust, TypeScript, and Kotlin in the same query.

<!-- end_slide -->

The argmax bookmark trick
---

No `MAX()` aggregate. Three rules: candidate, beaten, winner.

```bash +exec_replace +no_background
bat --color always --style=plain --paging=never --language=dl /Users/chrishafley/projects/sprefa/.claude/worktrees/vscode-flow-panel/deck/snippets/argmax.dl
```

<!-- pause -->

`latest_position` is every candidate that nothing later beats. This shape
resumes a review, a scan, or a chat session exactly where it left off, and it
falls out of plain negation.

<!-- end_slide -->

Ports and the lattice
---

`@in`/`@out` mark a relation's boundary. `key`/`merge` turn a relation into
a choice: one row per key survives.

```bash +exec_replace +no_background
bat --color always --style=plain --paging=never --language=dl /Users/chrishafley/projects/sprefa/.claude/worktrees/vscode-flow-panel/deck/snippets/ports.dl
```

<!-- pause -->

`--mcp` binds the `rpc` class to stdio and JSON-RPC. The class names the
contract; the transport is never in the `.dl` file, so the same program can
serve HTTP later without an edit.

<!-- end_slide -->

Diag rails and --check
---

`diag` is a reserved, fixed-schema built-in. A rule heading it is a CI-grade
lint rail.

```bash +exec_replace +no_background
bat --color always --style=plain --paging=never --language=dl /Users/chrishafley/projects/sprefa/.claude/worktrees/vscode-flow-panel/deck/snippets/diag.dl
```

<!-- pause -->

```bash
dl examples/banned-word-guard.dl --root . --check   # exit 2 on a hit
dl examples/banned-word-guard.dl --root . --lsp     # squiggles while editing
```

Same rule, three surfaces: CI gate, editor diagnostics, and an interactive
query.

<!-- end_slide -->

The flow panel / PR diff story
---

The VS Code flow panel is a webview wrapped around one query: point at a
function, walk who is downstream.

```bash +exec_replace +no_background
bat --color always --style=plain --paging=never --language=dl /Users/chrishafley/projects/sprefa/.claude/worktrees/vscode-flow-panel/deck/snippets/flowpanel.dl
```

<!-- pause -->

Open a PR, put the cursor on a changed function, and the panel re-runs this
closure live -- every caller, every field read, every JSX prop, across
languages, without re-scanning the whole repo.

<!-- end_slide -->

--move: refactor by query, not by grep
---

`--move` reads the SAME resolved module graph the flow panel reads, then
rewrites every `use` path that resolves through the moved file.

```bash
dl --move rust/kernel/clk.rs=rust/kernel/hw/clk.rs --fix --root .
```

<!-- pause -->

```rust
// before
use crate::clk::Clk;

// after (rewritten by dl, not by hand)
use crate::hw::clk::Clk;
```

<!-- pause -->

Brace-inner leaves, the moved file's own `super::` references, and the
physical file rename all ride the same pass. What the resolver can't
disambiguate, it counts out loud instead of guessing.

<!-- end_slide -->

Chat-marks: @@mark as a section header
---

The same argmax trick, one join deeper: attribute each message to the
nearest preceding mark.

```bash +exec_replace +no_background
bat --color always --style=plain --paging=never --language=dl /Users/chrishafley/projects/sprefa/.claude/worktrees/vscode-flow-panel/deck/snippets/chatmarks.dl
```

<!-- pause -->

A user message that starts with `@@mark Design pass` opens a new section;
every message after it, until the next mark, belongs to that section. No
Rust edit, no sidecar file -- the marker phrase is a plain string in this
program.

<!-- end_slide -->

Docs and the book
---

Every relation you have seen tonight is generated documentation, not a
hand-maintained list.

<!-- pause -->

* `README.md` -- the model, the DSL surface, the CLI, worked examples
* `examples/builtin-rels.dl` -- the catalog of every built-in relation, regenerated from the same `RelDecl` the engine typechecks against
* `dl setup --project` -- wires the maintainer skills and the doc generators into a fresh clone
* the book (`book/`) -- the long-form walkthrough, chapter per feature arc

<!-- pause -->

`dl_diag` lints `dl` itself, with the same engine that lints your code.

<!-- end_slide -->

sprefa dl
---

Facts in. Queries out. No bespoke server for any of it.

<!-- pause -->

The CLI, the LSP, the MCP server, and this VS Code panel are four surfaces
over one reactive engine.

<!-- pause -->

`dl --help`

<!-- end_slide -->
