# LSP topology archaeology

Status: research report. Implemented behavior is labeled `implemented`. Target topology is labeled `proposed`. Test evidence is labeled `tested`. The report was checked on 2026-09-18. No compiler or kernel behavior changes are included.

## Scope and source boundary

Research inputs:

- Current worktree.
- `/Users/chrishafley/projects/sprefa-archive-20260428`.
- `/Users/chrishafley/projects/sprefa-archive-20260701`.
- Named plans and session logs.
- `/Users/chrishafley/projects/sqlite_ivm`.

Path clarification:

- `../sqlite_ivm` is absent relative to this nested worktree.
- The repository exists at the workspace-level sibling `/Users/chrishafley/projects/sqlite_ivm`.
- The workspace-level sibling is the source used for the dependency comparison.

v1 evidence status:

- The v1 source tree is absent from the two archive roots.
- The v1 transport and feature path are recoverable from April chat logs and the v2 port.
- v1 claims are marked recovered rather than direct-source measurements.

System context:

- dl8 is a durable compiler runtime that emits code and imports types.
- It exposes a reactive IVM runtime emitter for queries over watched files and resources across codebases.
- The LSP is a transport surface over analysis state.
- Persistent disk facts and editor overlays have separate ownership.

## Version table

| version | techniques | transport | dependency reuse | file-change path | programmable hooks | process graph | tests and limits |
|---|---|---|---|---|---|---|---|
| v1 | Tree-sitter-backed tag and pattern analysis. Completion helpers handled tags, files, directories and partial source. Hover and diagnostics were assembled in the LSP crate. | `sprf-lsp` binary over stdio JSON-RPC. VS Code launched the binary through `vscode-languageclient`. Recovered from `chat_log/526ec0ed-RECOVERED.md` and `chat_log/20260331.7.sprf-lsp-highlighting-zig-builds.md`. | LSP owned a server-side view of scan/index/query state. The v1 material does not prove a shared daemon broker. | `didOpen` and `didSave` drove analysis. Editor file watching appeared in the VS Code extension. | Early programmable direction was LSP feature handling in the frontend. The later `lsp[capability]` operator is recorded as a v3 design direction, not a v1 wire contract. | VS Code extension -> one `sprf-lsp` process -> core scan/query code -> SQLite and cache layers. | `crates/sprf-lsp/tests/e2e/test_lsp.py` is present in the April archive. A session log records ten passing completion/diagnostic tests. The archive does not provide a retained-process soak or complete v1 teardown proof. |
| v2 | Tolerant partial parsing and strict parsing feed one runner. `DocSession` stores source, spans, diagnostics, reader and operator registry. `BufferOverlay` separates unsaved bytes from repository reads. | `sprefa_v2_lsp` used `tower-lsp` over stdio. The shared server also had LSP and HTTP transports. | `Reader`, `CheckoutLocator`, `GitBlobReader`, `ConfigLocator`, `InMemoryLocator`, `ResultStore`, and `BufferOverlay` are reused by analysis. Repo and revision identity are explicit in the reader/config layer. | `didOpen` creates or refreshes a per-URI session and overlay. `didChange` updates source and reparses. `didClose` removes the session and overlay keys in the transport layer. | `lsp[capability]` was a programmable design surface. The implemented v2 frontend dispatches registry operators and returns hover/completion/diagnostic facts. | One LSP process owns a URI map. The server architecture also described a daemon, watcher and LSP collapse into one process, with CLI as a SQLite client. | `v2/tests/lsp_smoke.rs` and the v2 LSP implementation are direct archive evidence. The code has bounded mutation channels such as `mpsc::channel(32)` and an HTTP graceful-shutdown path. `_2_lsp_layer.rs` stores subscriber senders by URI, so subscriber removal on connection close is a required teardown invariant. |
| v3 | Host parse plus injected trees. Operator registry dispatches hover, completion and diagnostics from grammar/node kind. LSP-facing operators were intended to own feature behavior. | `tower_lsp::Server` over stdio. An axum WebSocket bridge fed a `tokio::io::duplex` into the same LSP server. | `sprefa_parse`, pipeline operations, effect runtime, shared state and a server backend were reused. The WebSocket bridge was a transport adapter rather than an editor interoperability contract. | LSP notifications and a server-side filesystem watcher both fed shared state. This creates two change ingress paths that need deduplication and ownership rules. | `lsp[...]` operations were the explicit programmable hook. The server was a thin dispatcher over op-owned feature output. | stdio or WebSocket -> `tower_lsp` backend -> shared state and pipeline watcher -> parser/runtime/store. | Direct wire smoke scripts `_f_lsp_hover_ast.sh` and `_i_lsp_invalidate_kernel.sh` send Content-Length JSON-RPC to the real binary. `_k_lsp_diag.sh` tests diagnostic SSE rather than the LSP wire. The dual change ingress and WebSocket process boundary are limits. |
| v4 | Dedicated LSP crate. Semantic tokens, inlay hints, hover, completion, definition and debounced version-coalesced document changes. Tree-sitter and operator lowering were separated. | `tower-lsp` over stdio. VS Code extension launched `sprefa-lsp` and registered a `.sprf` file watcher. | `sprefa-lsp` reused the v4 parser and pipeline crates. The inlay implementation used one-shot parse/walk/expand and became dead after the fuser changed the pipe shape. | `didOpen`, `didChange` and `didClose` only in the LSP crate. Change debounce was 80 ms with version coalescing. | Operator registry and the planned LSP operations provided programmable feature ownership. Inlay was a server feature rather than a user-defined effect. | VS Code -> `sprefa-lsp` -> per-document analysis state -> parser/runtime. | `tests/lsp_hover_smoke.rs` and `tests/lsp_locate_dsl_smoke.rs` call handlers in-process through `build_in_process`, so they do not exercise JSON-RPC framing. `inlay_smoke.rs` is ignored and identifies dead inlay code. |
| v5 | Single compiler crate LSP. The server exposes definition, references, hover, symbols, call/type hierarchy, custom `dl/*` requests, commands and diagnostics. It uses disk-truth synchronization. | `lsp_server::Connection::stdio()` with `lsp-types 0.97`. The VS Code extension launches `sprefa-server --lsp-stdio`. | LSP attaches to the shared database daemon for `dl/diagChanged` and also has a `--diag-db` polling mode. The compiler and daemon own more state than the LSP session. | `didOpen` and `didSave` tick paths. `didChange` is not advertised; synchronization is `NONE`. A daemon subscription can push diagnostic changes. | Custom requests and commands are programmable protocol hooks, but the current LSP is a fixed Rust dispatch surface rather than user-authored feature code. | VS Code -> v5 stdio LSP -> compiler/daemon database -> diagnostic subscription or poll. | `docs/lsp.md` and `v6/tsv2/scripts/lsp_diag_driver.py` exercise Content-Length framing against the real binary. The v5 plan records a constraint against keeping the v5 binary running in the next design. |

## Version source anchors

These anchors sit below the central table so each row has a directly checkable path and line range.

| version | source anchors |
|---|---|
| v1 | Recovered implementation record: `/Users/chrishafley/projects/sprefa-archive-20260428/chat_log/20260331.7.sprf-lsp-highlighting-zig-builds.md:11-20,41-56`; recovered end-to-end test record: `/Users/chrishafley/projects/sprefa-archive-20260428/chat_log/20260411.20.lsp-completion-diagnostics-final.md:23-28,40-45`. |
| v2 | `/Users/chrishafley/projects/sprefa-archive-20260428/v2/src/bin/sprefa_v2_lsp.rs:1-22`; `/Users/chrishafley/projects/sprefa-archive-20260428/v2/src/server/_2_lsp_layer.rs:1-6,28-35,51-55,58-102,108-123`; wire test `/Users/chrishafley/projects/sprefa-archive-20260428/v2/tests/lsp_smoke.rs:1-25`. |
| v3 | `/Users/chrishafley/projects/sprefa-archive-20260701/v3/crates/sprefa/src/server/transport_lsp.rs:1-89`; `/Users/chrishafley/projects/sprefa-archive-20260701/v3/crates/server/src/state.rs:1-55`; existing measured table `plans/2026-08-12-lsp-archaeology.RESEARCH.md:59-69`. |
| v4 | `/Users/chrishafley/projects/sprefa-archive-20260701/v4/crates/sprefa-lsp/src/main.rs:123-189,192-235`; test `/Users/chrishafley/projects/sprefa-archive-20260701/v4/tests/lsp_hover_smoke.rs:1-25`; existing measured table `plans/2026-08-12-lsp-archaeology.RESEARCH.md:75-85`. |
| v5 | `src/lsp.rs:56-92,196-243,248-353,362-377,495-633` in the v5 source tree identified by the user; existing measured table `plans/2026-08-12-lsp-archaeology.RESEARCH.md:89-99`. Exit evidence: `plans/2026-07-29-finish-the-job-epic.md:916`; resident-swap evidence: `chat_log/20260722.2.v6-golden-data-1gb-cache-knob-mmap-redb-mermaid-map.md:5`. |

## v1 and v2 location receipt

The v1 implementation path is located through these recovered references: `crates/sprf-lsp/src/{main,completion,context,db,hover,workspace}.rs`, `crates/sprf-lsp/tests/e2e/test_lsp.py`, and the v1 VS Code launch instructions in `chat_log/526ec0ed-RECOVERED.md`. The source directory itself is absent from the checked archive roots. The v2 source is direct: `v2/src/bin/sprefa_v2_lsp.rs`, `v2/src/server/_4_transport_lsp.rs`, `v2/src/server/_2_lsp_layer.rs`, `v2/src/analysis.rs`, and `v2/tests/lsp_smoke.rs`.

## What the versions establish

Ownership-shape progression:

| phase | shape |
|---|---|
| v1 | One editor-spawned language process with analysis coupled to the LSP crate. |
| v2 and v3 | LSP attached to broader daemon or shared analysis state. v2 made the per-URI session and overlay explicit. v3 added a second filesystem change ingress. |
| v4 and v5 | Dedicated or folded LSP servers with increasing feature breadth. v5 added the disk-truth constraint and daemon diagnostic subscription. |

Programmable-LSP boundary:

- The programmable-LSP idea appeared before a stable topology.
- v3 expressed it as LSP operations dispatched over parsed nodes.
- A programmable effect requires explicit ownership of state, queue, cancellation and teardown.

## Leak evidence

| statement | classification | evidence |
|---|---|---|
| v1 LSP processes consumed all CPU in the background | recovered allegation | `chat_log/20260416.0.evaluator-store-mutation-design.md` records the user-past-bug. No retained process profile is present in the inspected sources. |
| v2 had a bounded mutation channel | implemented | `v2/src/analysis.rs` creates `tokio::sync::mpsc::channel(32)` and `_3_run.rs` creates a channel of 64. These bound queued mutation requests but do not bound analysis caches or external processes. |
| v2 close removes per-URI session and overlay state | implemented in transport layer | `v2/src/server/_2_lsp_layer.rs` has `close`, retained `overlay_keys`, and per-URI sessions. Subscriber cleanup on connection close must be checked at the call site. |
| v3 watcher and LSP notifications share change state | implemented | `v3/crates/server/src/state.rs` watcher plus `transport_lsp.rs` notification handlers. Duplicate event fencing is a topology requirement. |
| v5 process-exit retention | documented cause | `v5/src/lsp.rs:362-377` documents an exit hang because subscriber and poll-thread `Sender` clones retain the writer channel. `finish_lsp` bypasses `IoThreads::join`. `plans/2026-07-29-finish-the-job-epic.md:916` records one leaked hung `dl --lsp` per battery run. |
| v5 36 GB resident-swap | measured symptom with separate cause unresolved here | `chat_log/20260722.2.v6-golden-data-1gb-cache-knob-mmap-redb-mermaid-map.md:5` records the v5 memory result. The inspected evidence does not identify that resident growth as the writer-channel exit retention path. |
| v5 external-language-server process leak | unknown | The v5 evidence above concerns the v5 LSP process and its writer channel. It does not establish an external child language-server retaining path. |
| sqlite_ivm frees LSP memory | false inference | sqlite_ivm maintains SQLite rows, arrangements, prepared statements and virtual-table state. It does not own Rust task cancellation, bounded caches, child processes, or editor transport handles. |

Leak classes:

- Process-exit retention: live `Sender` clones and skipped `IoThreads::join`.
- Heap/store growth: the recorded v5 36 GB resident-swap result.
- Duplicated engines or caches.

Evidence status:

- Process-exit retention has a documented retaining path.
- Heap/store growth and duplicated engines require separate context and measurements.
- Database retraction can remove rows from an IVM view while process-local objects remain live.

## Sources

Repository and archive sources:

- `plans/2026-08-12-lsp-archaeology.RESEARCH.md`
- `plans/2026-08-12-v6-native-lsp.PLAN.md`
- `plans/2026-07-10-lsp-thin-client-daemon.md`
- `plans/2026-07-29-endurance-gate-noleak-brief.md`
- `/Users/chrishafley/projects/sprefa-archive-20260428/v2/src/bin/sprefa_v2_lsp.rs`
- `/Users/chrishafley/projects/sprefa-archive-20260428/v2/src/server/_2_lsp_layer.rs`
- `/Users/chrishafley/projects/sprefa-archive-20260701/v3/crates/server/src/transport_lsp.rs`
- `/Users/chrishafley/projects/sprefa-archive-20260701/v4/crates/sprefa-lsp/src/main.rs`
- `src/lsp.rs`

Primary upstream sources checked on 2026-09-18:

- [LSP overview and current specification](https://microsoft.github.io/language-server-protocol/): LSP is JSON-RPC between a development tool and a language server; 3.18 is current.
- [LSP 3.16 specification](https://microsoft.github.io/language-server-protocol/specifications/specification-3-16/): lifecycle, workspace, text synchronization and diagnostics sections. The fetched specification page is an older numbered specification, so current 3.18 feature details require the current specification selector.
- [rust-analyzer guide](https://rust-analyzer.github.io/book/contributing/guide.html): `AnalysisHost::apply_change`, cancellable `Analysis`, snapshots and Salsa query reuse.
- [rust-analyzer VFS discussion](https://rust-analyzer.github.io/blog/2020/05/18/next-few-years.html): immutable filesystem snapshots and transactional changes.
- [clangd index](https://clangd.llvm.org/design/indexing): dynamic file index, background thread pool, on-disk `*.idx` shards, static index and remote index.
- [TypeScript Language Service API](https://github.com/microsoft/TypeScript/wiki/Using-the-Language-Service-API): one language service per project, host-owned I/O and project dependency/reference resolution.
- [TypeScript language-service plugins](https://github.com/microsoft/TypeScript/wiki/Writing-a-Language-Service-Plugin): decorator plugins affect editing and are not loaded by normal command-line type checking or emit.
- [Volar.js architecture](https://github.com/volarjs/volar.js/): language-core virtual code, language-service features and language-server LSP transport are separate packages.

## sqlite_ivm reuse receipt

Dependency identity and tree:

- Path: `/Users/chrishafley/projects/sqlite_ivm`.
- Version: `0.3.0`.
- Dependencies: `rusqlite 0.40.2`, `sqlite3-parser 0.17.0`, `tracing`, `tracing-subscriber`, and `hafley-observe`.
- Exposed mechanisms: `extension::register(&Connection)`, source DDL functions, virtual-table registration and persistent maintenance through SQLite triggers.

The public LSP-relevant contract is a durable read model:

```rust
register(&Connection) -> Result<()>
CREATE VIRTUAL TABLE view USING sqlite_ivm('<SELECT ...>')
source INSERT | UPDATE | DELETE -> transactional view maintenance
SELECT * FROM view -> current materialized rows
```

Recursive maintenance:

- Source: `src/1a_relational.rs`.
- Private paths: `Plan::input` and `Plan::fixpoint`.
- Deletion records affected members in a work table.
- The derivable region is deleted.
- Semi-naive rowid ranges are rederived.
- Surviving rows are restored.
- The net removed delta is emitted.
- Tests cover savepoints, rollback, cascades, maintenance failure rollback and reopen.

Reuse candidates:

- SQLite transaction boundary.
- Persistent result rows.
- Source-trigger wakeup.
- Catalog ownership manifest.
- Queryable derived facts.

Missing contracts:

- Request cancellation.
- Bounded query result pages.
- Revision fencing.
- External-process ownership.
- Watcher teardown.
- Cache eviction.
- Per-workspace isolation.

The view layer stores relational state. It does not provide a broker lifecycle.

README restrictions:

- Readers and writers must load the extension.
- Writers must enable `recursive_triggers=ON` and `trusted_schema=ON`.
- Public result CRUD is rejected.
- Source tables must be ordinary `main` tables.
- Volatile functions are rejected.

LSP broker consequence:

- Source facts must be written through supported source tables.
- The broker must not mutate result tables directly.

## Validation receipt

Validation completed:

- Archive existence.
- v2 direct source.
- v3, v4 and v5 paths from the existing archaeology report.
- sqlite_ivm manifest and README.
- Recursive maintenance.
- Transaction tests.
- Upstream pages listed above.
- No build-heavy product tests were run.
- CI coverage is unchanged because only reports and a diagram are added.

Boop-Status: done
Validation: source spot checks complete; D2 render and file checks recorded in the handoff
