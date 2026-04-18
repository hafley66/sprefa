# sprefa v2

Cursor-pipeline evaluator for `.sprf` files.

Read `README.md` first.

## The rule

**A loop of N iterations should do O(1) expensive work, not O(N).**

Expensive = lock acquire, heap alloc, `.await`, SQL roundtrip, subprocess
spawn, git2 call, regex compile. Those happen once, outside. Inside the
loop is pointer math, slice index, HashMap probe, comparison, arithmetic.

The enforcement trick is **list programming at stage boundaries**: op
inputs and outputs are slices; per-item scalar methods don't exist on
the boundary, so N+1 is untypeable. Inside a stage, a worker loops
scalar over its slice — that's fine, the expensive resource is held
once per worker.

### Thread model (what fast tools actually do)

biome, ripgrep, swc, ast-grep all use **rayon work-stealing, not tokio**
on the scan hot path. Each worker holds its own per-thread state
(parser, file handle, scratch buffer); no shared locks inside the loop.
Tokio stays at the outer boundary (LSP stdio, cancellation, reparse
debounce) — one `spawn_blocking` opens a rayon scope, the scope does
all the per-file work, one oneshot returns results.

Shape:
```rust
// stage boundary: slice in, slice out
fn pipe(input: Box<[Cursor]>, ctx: &OpCtx) -> Box<[Cursor]> {
    input.par_iter()
        .map_init(
            || ctx.repo_pool.get(),        // one Repository per worker
            |repo, c| run_one(repo, c),    // full per-item work, sync
        )
        .collect::<Vec<_>>()
        .into_boxed_slice()
}
```

Scalar `run_one` lives *inside* the rayon closure, which is fine. The
point is no op exposes a `pipe_one(cursor)` method, so stage-to-stage
wiring cannot accidentally do N+1.

### Measured violations in this repo (2026-04-17, swc smoke)

- `src/readers/_2_git.rs:407, 475` — `Mutex<git2::Repository>` acquired
  inside `bytes()` and `blob_oid()`, called 1067× per scan. Note:
  `git2::Repository` is `!Sync`, so the Mutex is mandatory under the
  current "one shared handle" design. Fix direction: per-worker
  handles (cheap; libgit2 mwindow cache is process-global), not
  "remove the Mutex".
- `src/readers/_4_parse_cache.rs:203, 234, 311` — three
  `RwLock<HashMap>` write-locks per cache miss, each slot an
  `Arc<OnceCell<Arc<ParsedTree>>>`. Tree-sitter parse is idempotent;
  the OnceCell race protection is unjustified. Replace with `moka` or
  `quick_cache` keyed by oid.
- `src/ops/_9_ast_grep.rs:310` — `join_all` over 1067 per-cursor
  futures. Replace with rayon `par_iter` over the batch.
- `src/ops/_5_json.rs:414-422, 568-575` — `reader.bytes(...).await`
  inside `for fp in files`.

### Fast fixes before refactor (try these first)

- **`git_libgit2_opts(GIT_OPT_SET_CACHE_OBJECT_LIMIT, ...)`** at
  startup. libgit2's blob cache defaults to 0 bytes — every blob read
  pays full decode. A one-line config change may erase the
  "blob-read is slow" symptom without any architectural change.
- **Raise `GIT_OPT_SET_MWINDOW_FILE_LIMIT`** for repos with many
  packfiles.
- **Profile before refactoring**: 1067 files × 0.85ms release = 907ms
  total. Uncontended mutex acquisition is 50-200ns. The span log
  claims contention but arithmetic suggests the residual lives in
  `join_all`-quadratic-poll + parse-cache indirection, not the Mutex
  itself.

### Future moves (out of scope for immediate refactor)

- **gix migration**: `gix::ThreadSafeRepository` is `Send + Sync`;
  `.to_thread_local()` gives each worker a cheap handle. GitButler
  saw 2.6x from git2 → gix. Larger rewrite — revisit if git2
  per-worker-handles still bottleneck.
- **ast-grep `BitSet potential_kinds`**: ast-grep's own 10x speedup
  came from pre-filter-by-node-kind + multi-rule dedup traversal, not
  IO refactoring. Check whether sprefa matches multiple patterns per
  file and whether those coalesce into one tree walk.

### Reject in PRs

- `Mutex<Repository>` acquired inside a per-item call path.
- `.await` inside `for file in batch { ... }`.
- `join_all` / `FuturesUnordered` over per-file futures.
- `Arc<RwLock<HashMap<_, Arc<OnceCell<Arc<_>>>>>>` in new code.
- `sqlx` imports outside `src/store/`.
- Scalar `pipe_one(cursor)` style methods on op stage boundaries.
  Ops take slices.

### Patterns already in the codebase — keep using

- `Arc<[T]>` for cursor batches (one alloc, slice semantics).
- `Box<[T]>` for known-size owned immutable output. `Vec<T>` during
  growth only.
- Concrete enums when variants are bounded (`Pipeline`, `RunEvent`).
- Newtype wrappers over domain strings (`FilePath`, `ParseSite`,
  `Capture`).
- Ops own their diagnostics; each op file owns its `*Diag` type.
- One-owner batches on the rayon side of `op.ast` (`Vec<Prep>`, no
  per-item Arc).

---

Read `../chat_log/20260416.2.system-zoom-1-plus-golden-tests.md` for the
outer system (Reader / Store / MutationHandler / reparse) and the 10
golden tests that define acceptance. Note: the zoom-1 doc predates the
rules above — where it shows per-item methods on Reader/Store, treat
them as ghosts to exorcise, not specs to implement.

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
