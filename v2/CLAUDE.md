# sprefa v2

Cursor-pipeline evaluator for `.sprf` files.

Read `README.md` first. Then read `## LAWS OF MIN` below. Do not skip.

## LAWS OF MIN (non-negotiable, apply before writing any code)

The architectural boundary got destroyed once already. Reader, Store, and
MutationHandler grew per-item methods, per-item sub-traits, dispersed SQL,
three-tier RwLocks, mutex-serialized git calls, per-file futures, per-file
Arcs, per-file OnceCells. 1067 files = 1067 mutex acquisitions because the
trait *permitted* a scalar call. Every N+1 regression in this repo was
enabled by a singleton method on a trait that should have only taken
slices. LLMs cannot count — don't trust yourself to "remember to batch".
Make the scalar call un-typeable.

The constraint is min *everything*:

- min calls           — one method per cross-cutting trait
- min generics        — concrete types; no associated-type soup
- min locks           — one per wave, not per item
- min clone / copy    — batches own their bytes; clone at boundaries only
- min allocs          — `Box<[T]>`, not `Vec<T>`; pre-sized, no growth
- min arcs            — wave owns data; no per-item `Arc<OnceCell<Arc<_>>>`
- min heaps / stacks  — avoid futures-per-item; one future per wave
- min for loops       — prefer batch ops over per-item iteration where lock/alloc hops live inside
- min parens          — write plainly; no cathedrals
- min blocking        — one await per wave, not N
- min yolo            — instrument before claiming; stop guessing

### Trait shape the rules demand

```rust
trait Reader {
    fn read(&self, req: &ReadBatch) -> ReadResult;          // ONLY method
}
trait Store {
    fn apply(&self, batch: &WriteBatch) -> ApplyResult;     // ONLY method
}
trait MutationHandler {
    fn decide(&self, effects: &[Effect]) -> Box<[Decision]>; // ONLY method
}
```

- No `bytes()`, no `blob_oid()`, no scalar overloads.
- No `flush_batch` + `register_expr_schema` + `effect_cache` separation.
- A scalar caller constructs a length-1 batch. The type system enforces
  the wave model. N+1 becomes a compile error, not a performance footgun.
- SQL lives in exactly one module: inside the single `Store::apply` impl.
- Sub-traits are the smell. If a "per-item helper" is needed, it lives
  as a private free function inside the batch impl — never on the trait.

### Evidence of drift (do not re-commit these)

- `Reader::bytes(FilePath)` + `Reader::blob_oid(FilePath)` singletons →
  1067× `Mutex<git2::Repository>` acquisitions per scan (agent-verified
  2026-04-17, restart-warm debug run).
- `ParseCacheReader` holds THREE `RwLock<HashMap>` — primary + by_oid +
  by_hash — because per-item lookups each landed in their own lock.
- `Store` grew `register_expr_schema`, `flush_batch`, effect cache,
  scanner-hash set, DDL migration — five responsibilities, sqlx imports
  in multiple modules.
- `op.ast.prefetch` uses `join_all` over 1067 `prep_cursor` futures —
  a 1067-wide `FuturesUnordered` polled round-robin. Each future does
  6+ lock hops. Wave-batched `parsed_many` would collapse this to 4
  lock acquisitions total — but only if the underlying Reader is
  batch-shaped, otherwise the rewrite is cosmetic.

### Acceptance

A PR violates the LAWS if it:

1. Adds any scalar method to `Reader`, `Store`, or `MutationHandler`.
2. Uses `join_all` / `FuturesUnordered` / `buffer_unordered` over
   per-file futures.
3. Introduces `Arc<RwLock<HashMap<_, Arc<OnceCell<Arc<_>>>>>>` or any
   near-variant of that shape.
4. Adds `.await` inside a `for file in batch { ... }` loop. Use a bulk
   call.
5. Introduces sqlx usage outside the single `Store::apply` module.
6. Holds `Mutex<Repository>` inside a per-item call path.

If a proposed change conflicts with the LAWS, stop and rewrite the
trait first. The trait is the fix site; the caller is not.

---

Read `../chat_log/20260416.2.system-zoom-1-plus-golden-tests.md` for the
outer system (Reader / Store / MutationHandler / reparse) and the 10
golden tests that define acceptance. Note: the zoom-1 doc predates the
LAWS above — where it shows per-item methods, treat them as ghosts to
exorcise, not specs to implement.

This file is the minimum coherence surface for driving work in `v2/`.

## Outer shape (what README does not cover)

```
.sprf source
  │ host_parse() (tolerant for LSP, strict for CLI)        _8_parse.rs
  ▼
Vec<CursorExpr>    # ; delimited; named when rule(N) at head
  │ init_cursors(InitInputs)                                _7_init_cursors.rs  (Phase 2)
  ▼
BoxStream<RunEvent>    # Cursor / ExprDone / Diag / MutationPrompt / Done
  │
  ▼
consumers:
  bin/sprefa_v2   CLI; AutoApprove; print per cursor
  DocSession      LSP; RunReport collect; MutationPrompt → client code-action
  tests/stress.rs task count + RSS budget under reparse
```

Three layers of state outside the stream:

| Layer | Trait | Impls | Role |
|---|---|---|---|
| Reader | `readers::Reader` | `GitBlobReader`, `MemReader`, `BufferOverlay` | file-IO; 3-layer blob / worktree / buffer stack |
| Store | `store::Store` | `NoopStore` (P1), `SqliteStore` (P2) | per-expr rows, scanner_hash skip-set, effect cache |
| Handler | `mutations::MutationHandler` | `AutoApprove`, `InteractiveCli` (P2), `LspPromptBridge` (P2) | approval loop on TaskGuard; respawns on reparse |

Reparse discipline (`DocSession::on_source_change`):

```
cancel.cancel() → all op tasks bail → TaskGuard::drop → handler abort
fresh CancellationToken + fresh mpsc + respawn handler → reparse(new_src)
```

Bounded channels (`cfg.runtime.buffer_size`). Sequential repo processing. Cursors carrying content never cross op boundaries buffered. 16GB target for 500 repos.

## Five invariants (the spine)

1. **Ops own everything** — diagnostics, patterns, hover, fix, effect type. Framework holds `Arc<dyn Op>`, never switches on kind.
2. **Cursor is the unit of flow** — ops transform `BoxStream<Arc<[Cursor]>>`.
3. **Content contract** — every byte-reading op tries PATH A slot → PATH B `cursor.content[byte_range]` → PATH C `reader.bytes()`.
4. **Reads are pipe, writes are deferred effects** — mutations queue on mpsc, handler awaits approval.
5. **Reparse cheap, cancellation real** — `on_source_change` cancels the token; TaskGuard drop aborts.

A plan that violates these should pause and rethink.

## Current phase

Phase 1 landed on `wip/kitchen-sink-react-hook-fix` (dirty, pending commit):

- `store/` trait + `NoopStore`; `mutations.rs` trait family + `AutoApprove` + stub `InteractiveCli` + stub `LspPromptBridge`; `_task_guard.rs`
- `OpCtx` +store +mutations +cancel +expr_name +current_site
- `RuntimeConfig` +max_passes +max_claims_per_pass +max_cursors_per_root
- `RunEvent` rewrite (Cursor / ExprDone / Diag / MutationPrompt / Done); `CursorExpr`; `Pipeline: Clone`
- Test scaffold: `Config::test_default()`, `RuntimeConfig::test_default()`, `OpCtx::for_test(cfg, reader, writer)`; 16 test files collapsed

Phase 2 (next session's target): `SqliteStore` body + DDL + UDFs + migrations, `init_cursors`, `DocSession` rewire, `bin/sprefa_v2` collapse, `InteractiveCli` + `LspPromptBridge` bodies. Plan folder `chat_log/20260417.0.session2-phase2/` is pending draft.

Future sessions (one-liners per `feedback_scope_plans`): S3 watcher + cursor identity; S4 FTS5 trigram query layer; S5 write ops (render / write / marker_write); S6 import resolver + demand scanning; S7 daemon / LSP / CLI collapse; S8 invariant checks + remaining CLI. Each lands its own zoom-3 folder when its turn arrives.

## Golden tests (summary)

Full specs in `../chat_log/20260416.2.system-zoom-1-plus-golden-tests.md`.

- **G1** ast-grep fans out across tsx
- **G2** markdown + marker extracts TODO and fenced code
- **G3** json extracts scalar + nested `{$K: $V}` fan-out
- **G4** `${def.$NAME}` xref within repo
- **G5** cross-repo xref via scan-pointer FKs
- **G6** write op with approval (auto / cli / lsp)
- **G7** 1000 reparses, bounded RSS, flat task count
- **G8** schema evolution (DROP + rebuild on hash drift)
- **G9** mutation cache Skip / Stale / Emit
- **G10** LSP hover surfaces captures + effect preview

## Op conventions

- Numeric file prefixes = dependency order (`_0_rule.rs`, `_1_repo.rs`, ..., `_5_json.rs`, `_6_cursor_ref.rs`).
- Ops own diagnostics, patterns, hover, fix, effect type.
- Walker: `Leaf` scalar match, `CaptureAny` any node kind. Cross-refs use `Leaf` for constraint matching.
- Content contract: PATH A slot reuse → PATH B `cursor.content[byte_range]` → PATH C `reader.bytes()`. Order is strict.
- `Op::pipe()` returns `BoxStream<Arc<[Cursor]>>`. Never `Vec<Cursor>`.
- Mutation effects implement `MutationEffect`; framework holds `Arc<dyn MutationEffect>`.

## Build and test

```bash
cd v2 && cargo build --tests
cd v2 && cargo test -p v2
cd v2 && cargo test -p v2 --lib
cd v2 && cargo test -p v2 --test hover_render
```

Pre-existing failure out of scope: `doc_session_completion_fs_filter_glob_double_star`.

## Test scaffold (root convention echoed)

New field on `OpCtx` or `RuntimeConfig` → update `for_test` / `test_default`, never the test files. Compiler pointing at a test means the helper is the fix site.

- `Config::test_default()` — empty config; override via struct-update
- `RuntimeConfig::test_default()` — canonical runtime knobs
- `OpCtx::for_test(config, reader, writer)` — full plumbing (no-op diags/events, NoopStore, closed mutation channel, fresh cancel, empty ParseSite); override via `OpCtx { diags: my_sink, ..OpCtx::for_test(...) }`

Do not hand-write `RuntimeConfig { ... }` or the full 18-field `OpCtx { ... }` literal in a test.

## Project-local skills

`.claude/skills/`:

- `sprf-v2-op-trait-family` — Op / Operator traits, cursor/op ownership
- `sprf-v2-cursor-slots` — `Cursor.slots` + `SlotKey<T>` typed payload channel
- `sprf-v2-cursor-ref` — `&.$X` / `&.fs` / `&.repo` / `&.rev` desugar and rebase
- `sprf-v2-pipeline-tree` — dual-tree (pipeline vs content), PathSeg encoding
- `sprf-v2-content-contract` — byte-reading dispatch order
- `sprf-v2-port-op` — mechanical steps for adding a new op

## Key files (top of each layer)

```
v2/src/
  _0_types.rs          Cursor, Capture, CaptureKind, ParseSite, FilePath, Slots, SlotKey, RunEvent, CursorExpr
  _1_diagnostic.rs     Diagnostic trait, Renderer
  _2_config.rs         Config, RuntimeConfig, test_default()
  _3_reader.rs         Reader trait
  _4_writer.rs         Writer trait
  _5_op.rs             Op trait, Operator, Pipeline enum, LoweredOp, OpCtx, for_test()
  _7_runner.rs         (to collapse into _7_init_cursors.rs in Phase 2)
  _8_parse.rs          host_parse(), &. desugar, OpInvocation
  _task_guard.rs       TaskGuard<T>; Drop → abort
  analysis.rs          DocSession: hover_at, completions_at, span_ix
  mutations.rs         MutationEffect, MutationHandler, await_approval, AutoApprove, spawn_handler
  store/               Store trait, NoopStore (P1), SqliteStore (P2)
  ops/                 _0_rule .. _6_cursor_ref
  walk/                compile + walker for json/yaml/toml
  readers/             blob / WT / buffer stack
  writers/             MemWriter
  bin/
    sprefa_v2.rs       CLI driver
    sprefa_v2_lsp.rs   LSP driver
```
