# 1a — New modules

Independent new-file creation. Touches Cargo.toml + lib.rs for wiring;
no edits to existing source modules.

## Prereqs
None.

## Status
Landed. Spot-check on resume:
```
ls v2/src/_task_guard.rs v2/src/mutations.rs v2/src/store/
grep -E "async-trait|tokio-util|chrono" v2/Cargo.toml
grep -E "_task_guard|^pub mod store|^pub mod mutations" v2/src/lib.rs
```

## Files

### v2/Cargo.toml
Under `[dependencies]` add:
```
async-trait = "0.1"
tokio-util = { version = "0.7", features = ["rt"] }
chrono = { version = "0.4", features = ["clock"] }
```

### v2/src/lib.rs
After `pub mod _16_pattern;`:
```rust
pub mod _task_guard;
pub mod store;
pub mod mutations;
```
No re-exports at lib root from `store` or `mutations` — they collide with
existing `_4_writer::EffectStatus` and pollute the root namespace.

### v2/src/_task_guard.rs (new, ~35 LOC)
Full impl per `chat_log/20260416.0.evaluator-store-mutation-design.md`
Zoom-3 section. `TaskGuard(Option<JoinHandle<()>>)` with `spawn`,
`join`, `abort`, `Drop` calling `abort()`. Plus `noop()` associated fn
returning a guard with `None` (used as a placeholder for stub
`spawn_handler` in mutations.rs).

### v2/src/store/mod.rs (new)
```rust
pub mod _0_types;
pub mod _1_trait;
pub use _0_types::*;
pub use _1_trait::*;
```

### v2/src/store/_0_types.rs (new)
All of Z3 `src/store/_0_types.rs` pseudo: `ContentHash`, `Batch`,
`ExprBatch`, `RowInsert`, `Where`, `Row`, `EffectStatus`,
`EffectOutcome`, `EffectResult`, `StoreErr`.

Deviations from pseudo (Phase 1 only):
- `StoreErr::Sql` carries `String`, not `sqlx::Error` (sqlx not a dep yet)
- `StoreErr::Serde` carries `String`
- Skip `impl Diagnostic for StoreErr` — defer to Phase 4 when we decide
  how to attach a ParseSite to errors without source locations

### v2/src/store/_1_trait.rs (new)
Z3 pseudo for `Store` trait with `#[async_trait]`. Plus `ExprTableSpec`,
`CaptureColumn`.

Additionally add a `NoopStore` unit struct with `impl Store for NoopStore`
where every method body is `unimplemented!("NoopStore — Phase 2")`.
Constructor: `pub fn new() -> Self { Self }`. This is what the 5 existing
OpCtx construction sites instantiate so the tree compiles.

### v2/src/mutations.rs (new)
Z3 pseudo for `src/mutations.rs` with these Phase-1 deviations:
- `await_approval` body = `todo!("Phase 2")`
- `spawn_handler` body = `TaskGuard::noop()`
- `MutationHandler::handle` for each impl = `todo!("Phase 2")`
- `InteractiveCli::new()` returns a unit struct; no Mutex<BufReader>
  yet (tokio stdin wrapping deferred to Phase 2)
- `LspPromptBridge` holds `broadcast::Sender<RunEvent>` and forwards
  to `RunEvent::MutationPrompt` — this requires RunEvent from 1b. If
  1b not yet in, temporarily stub as unit struct with todo!() handle.

## Verify
```
cd v2 && cargo build --lib 2>&1 | tail -20
```
Should be clean. New modules are dangling — nothing references them
from existing code yet. That's the point.

## Exit state
- 3 new deps in Cargo.toml
- lib.rs declares 3 new modules
- TaskGuard fully functional
- Store trait + NoopStore + all store types exist
- MutationEffect + MutationHandler traits + 3 impls exist (stubbed)
- Existing code untouched
