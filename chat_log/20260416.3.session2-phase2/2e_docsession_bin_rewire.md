# 2e — DocSession + bin rewire

Switch `DocSession` and `bin/sprefa_v2` off the deleted `_7_runner` and
onto `init_cursors`. After this slice the tree builds clean with every
test (except `doc_session_completion_fs_filter_glob_double_star`) green.

## Prereqs

2c (init_cursors) + 2d (handler bodies). 2b for real SqliteStore, not NoopStore.

## Scope

```
v2/src/analysis.rs               DocSession::{new,on_source_change,ensure_run}
v2/src/bin/sprefa_v2.rs          run_cmd rewired to init_cursors
v2/src/bin/sprefa_v2_lsp.rs      LspPromptBridge wiring
```

## Files

### v2/src/analysis.rs — DocSession rewire

Clone Z3 section `### src/analysis.rs (DocSession rewire)` (lines
1383–1461 of design doc). Three methods:

- `DocSession::new` (Z3 lines 1386–1415):
  - add parameters: `store: Arc<dyn Store>`, `mutations_handler: Arc<dyn MutationHandler>`
  - inline `cancel = CancellationToken::new()`
  - inline `(mtx, mrx) = mpsc::channel(reader.config.runtime.buffer_size)`
  - inline `guard = spawn_handler(handler.clone(), mrx, cancel.clone())`
  - store all four on `self`

- `on_source_change(&mut self, new_source: String)` (Z3 lines 1417–1430):
  - `self.cancel.cancel()`
  - `drop(std::mem::replace(&mut self._handler_guard, TaskGuard::noop()))`
  - fresh `CancellationToken`, fresh mpsc, respawn handler
  - `self.last_run = None`
  - `self.reparse(new_source)`

- `ensure_run(&mut self)` (Z3 lines 1432–1446):
  - build `InitInputs` from self
  - `self.last_run = Some(collect_run_report(init_cursors(inputs)))`
  - `self.stale = false`

Struct field additions:
```rust
pub store:             Arc<dyn crate::store::Store>,
pub mutations_tx:      tokio::sync::mpsc::Sender<crate::mutations::MutationRequest>,
pub mutations_handler: Arc<dyn crate::mutations::MutationHandler>,
pub cancel:            tokio_util::sync::CancellationToken,
pub _handler_guard:    crate::_task_guard::TaskGuard,
pub exprs:             Vec<crate::_0_types::CursorExpr>,   # replaces pipelines
```

Remove `pipelines: Vec<(Arc<str>, Pipeline)>` — supplanted by `exprs`.

### v2/src/bin/sprefa_v2.rs — CLI rewrite

Clone Z3 section `### src/bin/sprefa_v2.rs` (lines 1463–1536). Three
shape changes vs today's bin:

1. `main` wraps in a tokio multi-thread runtime block-on
2. `run_cmd` constructs `SqliteStore::open(".sprefa.db")`, `MemWriter`
   (writer is mutation-target; okay for Phase 2), `AutoApprove`
3. For-loop over `outcome.cursor_exprs` calls `store.register_expr_schema`
   for named exprs before `init_cursors`
4. Main event loop consumes `init_cursors` stream and prints per variant

Remove any `_7_runner::run_pipelines(...)` call or equivalent.

### v2/src/bin/sprefa_v2_lsp.rs — LSP path

Analogous rewire. Construct `LspPromptBridge` with the server's
RunEvent sender. LSP's `DocSession` instance lives inside the LSP
handler, created on document open, reparsed on document change.

Key wiring:
```rust
let (events_tx, mut events_rx) = tokio::sync::mpsc::channel(128);
let bridge  = Arc::new(LspPromptBridge::new(events_tx.clone()));
let session = DocSession::new(src, reader, registry, store, bridge, writer);
// Spawn a task to drain events_rx and forward to the LSP client as
// custom notifications (MutationPrompt → code-action).
```

## Z3 deviations

- **MemWriter in CLI**: Z3 pseudo uses `MemWriter`. For Phase 2 keep
  it. Real filesystem write-through lands in S5 write ops session.
- **.sprefa.db path**: Z3 writes `root.join(".sprefa.db")`. Keep that
  default. Future flag `--db <path>` comes in S2 (config + discovery).
- **`host_parse` strict vs tolerant**: CLI uses strict
  (`host_parse_strict`), LSP uses tolerant (`host_parse`). Both already
  exist per `project_v2_parse_modes` memory. No deviation; just select
  the right one at call site.

## Callsite fixups

From 2c inventory, rewire each `_7_runner::` call. Expected sites:

- `v2/src/analysis.rs:*` — the `ensure_run` cluster, plus test-only
  paths in the same file
- `v2/src/bin/sprefa_v2.rs:*` — the run command dispatch
- Any test file that imports `_7_runner::...` — replace with
  `_7_init_cursors::init_cursors` or `collect_run_report`

For test-only DocSession construction (e.g. `tests/doc_session.rs`),
update to pass `SqliteStore::open_memory().await.unwrap()` and
`Arc::new(AutoApprove)`. Hide the async setup behind a test helper
`docsession_for_test(source)` in a fresh `v2/src/test_support.rs`
module (or extend `OpCtx::for_test`'s neighbor).

## Verify

```
cd v2 && cargo build --tests
cd v2 && cargo test -p v2
```

Full suite green except the pre-existing
`doc_session_completion_fs_filter_glob_double_star`. Golden tests
G1–G5 now runnable via the CLI.

## Exit state

- `_7_runner.rs` deleted, nothing references it
- `DocSession::new` takes `store` + `mutations_handler`
- CLI binary runs a .sprf end-to-end against SqliteStore
- LSP binary wires RunEvent → client notifications
- G1–G5 demonstrably work (`cargo run -- examples/hooks.sprf` prints rows)
