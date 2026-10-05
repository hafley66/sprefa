# Plan: LSP comes back as dl8 hosted executors (backlog, one at a time)

User, 2026-10-05. Ideas captured in order given; each becomes its own lane later.
The v5-and-earlier LSP inventory is in flight; its tables get appended here.

## Goal

A programmable LSP: dl7 rules answer "this OpenAPI config / type graph changed, who is
affected (files, repos, call sites)" efficiently, and the LSP serves those answers
(diagnostics, hover, code lens). AI steering reads the same rule outputs.

## Items

| # | item | today | notes |
|---|---|---|---|
| 1 | LSP features as comptime/runtime specials, same seam as `fs.watch` | `fs.watch` Continuing executor, `run_live` one engine, signed `poll_signed` | `didChange` = signed source; `on_settle` deltas -> `publishDiagnostics` |
| 2 | Bring back v5-and-earlier LSP and comment-node PKM; v5 goldens are acceptance cases | dropped when dl8 became a generic IVM compiler | inventory pending |
| 3 | Comment nodes / PKM come from sprefa-extract (ryi), not dl8 parsing | ryi extraction | |
| 4 | Rule sources: ast-grep, TypeSpec, OpenAPI, type info | `extract` executor; `oai.document`; TypeSpec loader not built; ryi type plane | take over the LSP concept from ast-grep/biome one tool at a time; may source from them |
| 5 | Executor API declared in TypeSpec; generate std `*.dl7` decls, Rust stub + dispatch, wire types | ryi `schema/cli/ops.tsp` -> `2_gen.mjs` -> `server_auto.rs` | removes relation-name string dispatch in `_3_executors/mod.rs` |
| 6 | External executors over a generic protocol | `fetch_json`; ABI cost table `book/src/hosting/2_adding_boop.md`; PRs #765, #766 | candidate table first (OpenAPI+SSE, JSON-RPC stdio, MCP, wasm/WIT); no bespoke |
| 7 | Host-or-client shape for every tool (LSP, rust-analyzer, ryi): build feature selects full in-process vs client-only; when a daemon runs, connect, never embed | ryi plan: trait Ryi + DaemonClient / SpawnClient / InProc(feature) | same shape for the LSP server |
| 8 | Rename executor trait vocabulary (`Cadence`, `Once`, `Continuing`, `armed`, `poll`, `poll_signed`, `answer`, `cancel`) | names rejected by user | later, with `ryi rename` |

## Prior code

- `~/projects/tiny-serve`: 806 lines, smol + lsp-server + lsp-types + rusqlite; HTTP, WS, LSP
  (Initialize, Hover, Shutdown); one store writer task.
