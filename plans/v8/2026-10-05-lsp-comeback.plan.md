# Plan: LSP comes back as dl8 hosted executors (backlog, one at a time)

User, 2026-10-05. Ideas captured in order given; each becomes its own lane later.
Order: this session finishes the core (comptime, macrotime, IVM, TypeSpec features); everything
here starts after that.
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
| 9 | Each executor runs on its own thread; the engine core is executor-agnostic | executors polled from the tick loop (`_2_reconcile.rs` `poll_signed` time slices) | depends on the IVM core work first |
| 8 | Rename executor trait vocabulary (`Cadence`, `Once`, `Continuing`, `armed`, `poll`, `poll_signed`, `answer`, `cancel`) | names rejected by user | later, with `ryi rename` |

## Prior code

- `~/projects/tiny-serve`: 806 lines, smol + lsp-server + lsp-types + rusqlite; HTTP, WS, LSP
  (Initialize, Hover, Shutdown); one store writer task.

## Inventory v1-v5 (read-only agent, 2026-10-05)

Paths relative to this repo unless absolute. v4/v3: `~/projects/sprefa-archive-20260701/`;
v2/v1: `~/projects/sprefa-archive-20260428/`. Prior archaeology:
`plans/2026-08-12-lsp-archaeology.RESEARCH.md:50-110`, `plans/v8/lsp-topology-2026-09-18/`.

### Acceptance goldens (v5 `v5/tests/it/`)

| area | files | count |
|---|---|---|
| LSP protocol | lsp_protocol.rs, lsp_refs.rs, lsp_symbols.rs, lsp_hierarchy.rs, lsp_hook_event.rs, lsp_query_paging.rs, lsp_exit.rs, diag_mute.rs | diagnostics, definition, references (resolved/textual/compiler tiers, cross-repo), hover (+ type profile), highlight, document/workspace symbols, call + type hierarchy, `dl/query` paging, `dl/hookEvent`, exit codes, diag mute |
| PKM | hover_note.rs, comment_node.rs, doc_comment.rs, comment_op.rs (7), chat_marks.rs, suppress.rs (9) | comment rows per language, doc comments + tags, marker regions, `@@mark`, suppression directives |
| impact | xrepo_cycle.rs, flow_services.rs, changed.rs | cross-repo cycle, value across wire via OpenAPI, changed set joined with a rail |

### v5 LSP shape

- Features: definition, references, hover, documentHighlight, documentSymbol, workspace/symbol,
  call + type hierarchy, executeCommand (`dl.toggleDiagCode`, `dl.listDiagCodes`), custom
  `dl/refs`, `dl/locate`, `dl/query`, `dl/hookEvent`, publishDiagnostics. No completion.
  `v5/src/lsp.rs:57-92` caps, `:247-320` dispatch.
- Data: `diag`/`diag_mute`; `def_target` -> `module_edge`; hover from `type_entity`, `call_*`,
  `type_edge`, `hover_note`; refs via SCIP -> resolved -> textual tiers (`engine/lens.rs`).
- Updates: save-driven, disk truth. `change: NONE`; didOpen/didSave -> `tick_paths(path)` ->
  republish; daemon push `dl/diagChanged`; cold full tick at initialize.
- v4 had didChange (80 ms debounce, stale-version drop), VFS overlay, Watchman pre-warm,
  `MEMO_DEPS.by_path` reverse index (branch `feat/lsp-diags-to-claude-code`, unmerged, 12 commits).

### PKM relations (v5)

`comment_node`, `comment(...)` marker op, `doc_comment`/`doc_tag`, `note(tag,msg,file,line)` from
`<!-- @plan ... -->`, `plan_todo`/`code_todo`, `hook_event` -> `doc_mark`/`section_of`,
`note`/`link_note` -> stale diag, `suppressed`/`suppress_span`/`suppress_unused`, `hover_note`.
No wiki-link or `@`-mention relation in any version.

### Impact examples (v5 `examples/`)

`openapi-lsp.dl` (operationId -> TS call / Rust def, `orphan-op`), `openapi.dl`,
`flow-services.dl`, `endpoint-flows.dl`, `typegraph.dl` (type_edge closure WORK vs HEAD),
`pin-skew.dl`, `phantom-deps.dl`, `pr-diff.dl`, `module-history.dl`.

### Library choice already made

`docs/ext-lsp-made-easy.md:224-243`: `lsp-server` 13 vs tower-lsp / async-lsp 2-3.

### v8 today

No LSP code. Crosswalk `plans/v8/2026-09-26-crosswalk.md:44` R32 missing; `:184-190` maps the v5
`lsp_*` tests.

### Watchers by version

v1 notify 100 ms; v3 notify_debouncer_mini + gitignore; v4 git_watch domains, Watchman (branch);
v5 daemon per-root notify + `.git` watch, quiet 120 ms / max 600 ms (`v5/src/daemon/shell/watch.rs`),
`watchgate.rs`, HEAD poll example. v8: `fs.watch` 100 ms debounce, `git.refs`.
