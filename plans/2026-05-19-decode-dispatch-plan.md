# Decode and dispatch hardening plan — effect_runtime — 2026-05-19

Targets `v3/crates/effect_runtime/` panic and `Any`-downcast surface called out in `plans/2026-05-19-v4-worst-audit.md` items 6 through 10 plus the "default trait impls that lie" amplifier. Output is a sequence of PRs that turn each panic site into a typed `Result` or a compile-time impossibility, without shedding the v4 consumer.

## 1. Ordering rationale

Decode first, dispatch second, default-impl tightening third.

1. Codec is leaf. Touching `v2/codec.rs` ripples through `sqlite_queue.rs`, `runtime_bridge.rs`, `cursor_codec.rs`. No structural surface change to `RtCtx` or `ExpandOpts`. One PR with mechanical migration.
2. Dispatch (`Any` → typed) touches `RtCtx`, `ExpandOpts.runtime`, `ExpandOpts.memo_seam`, `Lineage`, `Store`. Wider blast radius. Easier after codec PR proved migration shape.
3. Default impls largely a `queue.rs` audit. Follows because required-method conversions need real impls in `sqlite_queue.rs` first.
4. `bounded_batched.rs` worker-panic and `cst/locate.rs` panic are small follow-ups.
5. `wake_kind` `repr(i64)` enum rides with codec PR (row decoder is the only consumer).

Six PRs total. None mandatory-sequential except codec before dispatch.

## 2. PR-1: `Codec` returns `Result`, `wake_kind` becomes enum

Files: 6 in v3, 2 in v4.

### 2.1 New trait shape

```rust
#[derive(Debug, thiserror::Error)]
pub enum DecodeError {
    #[error("short read: need {need} bytes, got {got}")]
    Short { need: usize, got: usize },
    #[error("invalid utf8 at offset {offset}: {source}")]
    Utf8 { offset: usize, source: std::str::Utf8Error },
    #[error("invalid discriminant for {field}: {value}")]
    BadDiscriminant { field: &'static str, value: i64 },
    #[error("trailing bytes: {count}")]
    Trailing { count: usize },
    #[error("decoder rejected: {0}")]
    Custom(Cow<'static, str>),
}

pub trait Codec: Sized {
    fn encode(&self) -> Vec<u8>;
    fn decode(bytes: &[u8]) -> Result<Self, DecodeError>;
}
```

No default impl. Old `decode` signature deleted. Every impl must opt in. Migration cudgel.

### 2.2 Built-in impls

`Codec for i64`, `Codec for u64`, `Codec for String` rewritten bounds-checked.

```rust
impl Codec for u64 {
    fn encode(&self) -> Vec<u8> { self.to_le_bytes().to_vec() }
    fn decode(b: &[u8]) -> Result<Self, DecodeError> {
        let a: [u8; 8] = b.get(..8)
            .ok_or(DecodeError::Short { need: 8, got: b.len() })?
            .try_into().unwrap();
        if b.len() != 8 { return Err(DecodeError::Trailing { count: b.len() - 8 }); }
        Ok(u64::from_le_bytes(a))
    }
}
```

Trailing-byte enforcement opt-in per-impl. Numeric impls enforce; `String` does not; `Cursor` does not.

### 2.3 `wake_kind` enum

`v3/crates/effect_runtime/src/v2/sqlite_queue.rs:76-78`:

```rust
#[repr(i64)]
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum WakeKind { Immediate = 0, Tick = 1, Key = 2 }

impl TryFrom<i64> for WakeKind {
    type Error = DecodeError;
    fn try_from(v: i64) -> Result<Self, DecodeError> {
        match v {
            0 => Ok(WakeKind::Immediate),
            1 => Ok(WakeKind::Tick),
            2 => Ok(WakeKind::Key),
            other => Err(DecodeError::BadDiscriminant { field: "wake_kind", value: other }),
        }
    }
}
```

`row_to_queue` becomes `fn row_to_queue<N: Next + Codec>(r: &Row) -> Result<QueueRow<N>, RowDecodeError>`. Each `wake_key.unwrap()` / `wake_tick.unwrap()` gets `.ok_or(DecodeError::Custom(...))?`. 32-byte check on `wake_key`:

```rust
let blob = wake_key.ok_or_else(|| DecodeError::Custom("Key row missing wake_key".into()))?;
if blob.len() != 32 {
    return Err(DecodeError::Short { need: 32, got: blob.len() });
}
let mut k = [0u8; 32]; k.copy_from_slice(&blob);
```

### 2.4 Propagation into `pull_runnable`

Keep trait stable. Add internal `try_pull_runnable_row -> Result<Option<_>, RowDecodeError>` on `SqliteQueue` only. `pull_runnable` calls it; on `Err`:

- emit `Diag::Error` via bus
- delete corrupt row by `id`
- recurse to next runnable

Document: implementations may skip rows that fail decode and emit diagnostics; missing rows must not cause silent loss for non-corrupted rows.

### 2.5 `runtime_bridge.rs` expect

Today: `.expect("valid cursor codec bytes")`. After: `Result<Self, DecodeError>`. `cursor_codec::decode` already returns `Result<Cursor, &'static str>`. Bonus fix: turn missing FOCAL re-injection at `cursor_codec.rs:130-132` into `Err("focal term absent")` — audit Tier-0 item 4 bundled here cheaply.

### 2.6 Test strategy

- Round-trip property test in `codec.rs` with `proptest`. Mutate one byte; assert decode is `Err` for every fixed-width type.
- Unit in `sqlite_queue.rs`: write a row with `wake_kind = 99`, assert `pull_runnable` returns the next valid row, asserts one diagnostic emitted, asserts corrupt row gone.
- Unit on `cursor_codec`: truncated buf at every prefix length, assert `Err`.
- Integration on `v4/src/dirty_source.rs`: malformed queue blob does not crash expand.

### 2.7 Rollback

Single trait signature change. `git revert -m1` is the rollback. No on-disk schema change.

## 3. PR-2: dispatch surface

Three `Any` users, three best-fit answers:

| Site | Decision | Why |
|---|---|---|
| `RtCtx.registry` `TypeId -> Box<dyn Any>` (`lib.rs:112-230`) | **Keep `Any`, surface `Result`** | Open-world by design. |
| `RtCtx.stores` `Option<Arc<S>>` (`lib.rs:329`) | **`Result<Arc<S>, StoreError>`** with `Absent` vs `Mismatched` | Option collapses two failure modes. |
| `ExpandOpts.runtime` `Option<Arc<dyn Any>>` (`expand.rs:112`) | **Generic slot** | One consumer (sprf). |
| `ExpandOpts.memo_seam` (`expand.rs:114`) | **Generic in N** | SeamCell hack disappears. |
| `Lineage = Arc<dyn Any>` (`subjects.rs:102`) | **`LineageTag` marker** | Narrow the seal. |

### 3.1 Stores: shape

```rust
#[derive(Debug, thiserror::Error)]
pub enum StoreError {
    #[error("store not registered: {type_name}")]
    Absent { type_name: &'static str },
    #[error("store registered under {type_name} but Arc downcast failed (framework bug)")]
    Mismatched { type_name: &'static str },
}

impl RtCtx {
    pub fn store<S: Store>(&self) -> Result<Arc<S>, StoreError> { ... }
    pub fn try_store<S: Store>(&self) -> Option<Arc<S>> { self.store().ok() }
}
```

Migration: `cx.store::<X>().expect(...)` → `cx.store::<X>()?` or `unwrap_or_else(panic!)` with explicit message. Grep `store::<` to enumerate v4 sites.

### 3.2 ExpandOpts<N>

Becomes generic in `N: Next`. `expand::<N>` already requires that bound. `SeamCell` deletes. Breaking for v4: `ExpandOpts::default()` becomes `ExpandOpts::<Cursor>::default()`. Usually inference fills it from the queue.

### 3.3 Effect registry: keep Any, narrow the panic

```rust
let typed: E = *req.downcast::<E>().unwrap_or_else(|_| {
    panic!(
        "BUG: BatcherEntry<{}> received non-{} payload. Registry insert path \
         is the only insertion site; reaching this means the framework was \
         modified to allow heterogeneous insertion under one TypeId.",
        std::any::type_name::<E>(), std::any::type_name::<E>(),
    )
});
```

For `put` at lib.rs:218, add `try_put` returning `Result<_, PutError>`. Existing `put` stays as `try_put().unwrap_or_else(panic!)`.

### 3.4 Lineage marker

```rust
pub trait LineageTag: std::any::Any + Send + Sync + 'static {
    fn as_any(&self) -> &dyn std::any::Any;
}
impl<T: std::any::Any + Send + Sync> LineageTag for T {
    fn as_any(&self) -> &dyn std::any::Any { self }
}
pub type Lineage = Arc<dyn LineageTag>;
```

Predicate becomes `FnMut(&dyn LineageTag) -> bool`. Marker only; no behavior change.

### 3.5 Files (estimate 8)

- `v3/.../lib.rs` (RtCtx, Store, StoreError, PutError)
- `v3/.../subjects.rs` (LineageTag)
- `v3/.../v2/expand.rs` (ExpandOpts<N>, drop SeamCell)
- `v3/.../v2/component.rs` (RenderCtx runtime typing)
- `v4/src/{dirty_source,runtime_replay,chan,term,fact,template}.rs` (turbofish)
- `v4/src/app.rs` (`store::<X>` migration)

### 3.6 Test

- Unit: `try_put` without registration → `PutError::NotRegistered`.
- Unit: wrong-type store fetch → `StoreError::Absent`.
- Compile-fail doc test: `ExpandOpts::<Cursor>::default().memo_seam = Some(other_type)` does not compile.

### 3.7 Rollback

`ExpandOpts<N>` is the irreversible piece. Bundle `pub type ExpandOptsAny = ExpandOpts<DefaultNext>` in PR-2 so the rollback path stays open.

## 4. PR-3: default trait impls that lie

`v3/.../v2/queue.rs:122-188`.

| Method | Today | Verdict | Shape |
|---|---|---|---|
| `dispatch_park` | `0` | silent no-op | **required** |
| `has_parked_domain` | `true` | inverts optimization | **opt-in via marker** |
| `pending_summary_before_or_at` | zeros | premature flush | **required** |
| `cascade_delete` | `0` | silent leak | **required** |

### 4.1 Two-trait split

```rust
pub trait QueueBackend<N>: Send + Sync {
    fn enqueue(&self, row: QueueRow<N>) -> QueueId;
    fn pull_runnable(&self, t: ExpandTick) -> Option<QueueRow<N>>;
    fn depth(&self) -> u64;
    fn dispatch_park(&self, domain: &str, key: Option<NextKey>) -> u64;
    fn pending_summary_before_or_at(&self, ...) -> PendingSummary;
    fn cascade_delete(&self, root: QueueId) -> u64;
}

pub trait IndexedParkLookup<N>: QueueBackend<N> {
    fn has_parked_domain(&self, domain: &str) -> bool;
}
```

Fallback: change default to `false` (pessimistic-safe). Document. One-liner. Evaluate trait split as a separate cleanup PR.

### 4.2 Required methods

`dispatch_park`, `pending_summary_before_or_at`, `cascade_delete` lose defaults. `MemQueue`, `SqliteQueue` already implement all three.

### 4.3 Test

- Compile fail: `DummyQueue` with only `enqueue`/`pull_runnable`/`depth` must not compile.
- Behavioral: `has_parked_domain=false` default; assert dirty events still wake parked rows.

### 4.4 Files (estimate 4)

- `v3/.../v2/queue.rs`
- `v3/.../v2/mem_queue.rs` (verify)
- `v3/.../v2/sqlite_queue.rs` (verify)
- `v3/.../v2/hybrid_queue.rs` (verify)

## 5. PR-4: bounded_batched worker survival

`v3/.../batchers/bounded_batched.rs:68-72`. `assert_eq!` kills worker; pending replies silently dropped.

Minimum honest fix:

1. Replace `assert_eq!` with `tracing::error!` + diag emit.
2. `outs.len() < replies.len()`: drop extra reply senders.
3. `outs.len() > replies.len()`: drop extra outs.
4. Keep worker alive.

Caller side at `:96` does `rrx.await.expect("batched reply dropped")` — that becomes second-order panic. Change `Batcher::run` to return `Result<E::Response, BatcherError>`. About six in-tree call sites.

Files (estimate 4): `lib.rs`, `bounded_batched.rs`, `cache_layer.rs`, v4 impls.

## 6. PR-5: locate / path registry consolidation

`v4/src/cst/locate.rs:193` panics; `path.rs:77` has the same registry. Move `clone_box` onto trait as required:

```rust
pub trait PathItem: Send + Sync + std::fmt::Debug {
    fn kind_tag(&self) -> &'static str;
    fn as_any(&self) -> &dyn std::any::Any;
    fn dyn_eq(&self, other: &dyn PathItem) -> bool;
    fn clone_box(&self) -> Box<dyn PathItem>;
}
```

Each item implements `Box::new(self.clone())`. Match statements at `path.rs:77` and `locate.rs:187` delete. `panic!` at `locate.rs:193` becomes `unreachable!` (now type-guaranteed).

Files (estimate 3): `path.rs`, `locate.rs`, item impls under `cst/items/`.

## 7. Migration story

| PR | Breaks v3 tests? | Breaks v4? | Cost |
|---|---|---|---|
| 1 codec | yes (tests.rs:77) | yes (runtime_bridge.rs) | low |
| 2 dispatch | yes (ExpandOpts) | yes (~6 turbofish sites) | medium |
| 3 default impls | yes if test backend | low | low |
| 4 batcher Result | yes (any Batcher) | yes (any v4 batcher) | medium |
| 5 PathItem clone_box | no | yes (4 items) | low |

Per PR: land in single atomic commit covering both crates (one workspace). CI: `cargo check -p effect_runtime` + `cargo check -p sprf-v4` + `cargo test --all`. No deprecation shims (both pre-1.0).

Exception: `ExpandOpts<N>` worth a `pub type ExpandOptsAny = ExpandOpts<DynNext>` shim for one release.

## 8. Estimate

~26-28 files across five PRs. No PR exceeds 10 files. Net lines: -200 deleted (panic code), +400 added (Result wiring + tests).

## 9. Cross-cutting tests

- Panic-property test: `nextest --test-threads=1` with custom panic hook that fails on ANY non-`#[should_panic]` panic.
- Blob-corruption fuzz target on `Cursor::decode` and `SqliteQueue::row_to_queue` (cargo-fuzz). 1 min in CI.
- LSP smoke test in v4: open corrupted persisted queue, assert daemon survives.

## 10. Rollback summary

Per-PR `git revert`. No sqlite schema changes (only Rust decoder shapes), so old binary still reads dbs written by new binary.

`ExpandOpts<N>` is the one irreversible-in-practice change. Bundle the legacy type alias to preserve rollback.
