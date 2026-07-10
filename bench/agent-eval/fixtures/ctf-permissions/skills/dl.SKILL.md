# Working in this repository — tooling guide

You have the standard shell tools (`bash`, `grep`/`rg`, `glob`, file `read`)
AND a set of **`dl` code-graph tools** exposed over MCP. `dl` has already
indexed this repository as a queryable graph of definitions, calls, types, and
data flow. Prefer the graph tools for "who calls / where defined / what
references" questions — they resolve indirection (constants, enums, wrapper
functions) that plain text search misses. There is no build step required to
answer; do NOT run `cargo`/`tsc`/`npm`.

## The codebase

A small permission-gated service with a **TypeScript** app (`app-ts/`) and a
**Rust** app (`app-rs/`). Permission enforcement is spread across both.

## The dl tools

- **`dl.what <name>`** — everything dl knows about a symbol: its definition
  site(s) with `file` and `line`, and its caller/callee counts. Your first
  move for any named function, type, or field.
- **`dl.verb <verb> <name>`** — concept queries. The two verbs:
  - `who-calls <name>` → every caller of the function `<name>`, each with
    `caller`, `file`, `line`. This is how you enumerate gate sites when
    enforcement is wrapped in a helper (a guard, a service method, a config
    check): ask who-calls that helper.
  - `where-defined <name>` → definition sites across the type/call graph.
- **`dl.rows <rel>`** — dump a whole relation. Useful ones:
  `call_edge` (caller→callee across the codebase), `call_def` (function
  definitions), `type_entity` (declared types/functions with file+line),
  `df_node` (data-flow nodes). Filter the rows yourself.

## Suggested method (count first, then verify)

1. **Find the concept's anchors.** `dl.what` the obvious names
   (`check_export_flag`, `require_export`, `enforce_rule`, `permissions`, and
   the `PERM_EXPORT` / `Permission::CanExport` constants).
2. **Enumerate gate sites via the call graph.** For each enforcement helper,
   `dl.verb who-calls <helper>` lists every site that invokes it — that is the
   set of gates for that abstraction, with `file` and `line`, without grepping
   for a literal that isn't there. Cross-check counts against `dl.what`'s
   caller count.
3. **Cover all abstractions.** The same permission may be gated as a raw flag
   check, a service `require` call, a guard/middleware wrapper, and a runtime
   config rule — each has its own helper to trace.
4. **Reject decoys.** A different concept (e.g. the import permission) or a UI
   toggle will have its own, separate helpers and callers; don't merge them.
5. **Both languages.** dl indexes `app-ts/` and `app-rs/` together; check both.

## Answer format

Report the 1-based `line` dl gives you. Follow the exact JSON output shape the
question asks for. Count your sites before answering — missing a gate hurts
recall, listing a decoy hurts precision.
