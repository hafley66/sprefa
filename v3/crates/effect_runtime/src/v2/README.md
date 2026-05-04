# `effect_runtime::v2` — pull-based react-shaped runtime

A queue-backed runtime for **streaming, content-addressed, durable** value pipelines.
Generic over the carrier (the value flowing through). Components return
descriptions of what should happen next; the driver executes. Wake and
cache-invalidation share one mechanism (`EventBus`). Sqlite-backed for
redux-persist semantics across process restart.

The vocabulary deliberately matches React / Redux / react-query / redux-saga / RxJS
since the substrate maps cleanly onto each idiom.

---

## Table of contents

- [Mental model in one screen](#mental-model-in-one-screen)
- [The seven core types](#the-seven-core-types)
- [Example 1 — smallest possible pipe (i64 doubler)](#example-1--smallest-possible-pipe-i64-doubler)
- [Example 2 — a real carrier (`LabCursor`) with `content_hash`](#example-2--a-real-carrier-labcursor-with-content_hash)
- [Example 3 — fan-out via `Node::Many`](#example-3--fan-out-via-nodemany)
- [Example 4 — `Suspense` parks the row at `pc+1`](#example-4--suspense-parks-the-row-at-pc1)
- [Example 5 — bus dispatch from a background thread (no async runtime)](#example-5--bus-dispatch-from-a-background-thread-no-async-runtime)
- [Example 6 — `Yield` re-renders the SAME component](#example-6--yield-re-renders-the-same-component)
- [Example 7 — saga-style effects: `EffectDispatch` + `Spawner`](#example-7--saga-style-effects-effectdispatch--spawner)
- [Example 8 — `useMemo` (`Memoize<C>`) with domain invalidation](#example-8--usememo-memoizec-with-domain-invalidation)
- [Example 9 — `useQuery` (`Query<N, F>`) with `invalidateQueries`](#example-9--usequery-queryn-f-with-invalidatequeries)
- [Example 10 — `SqliteQueue` + crash-restart](#example-10--sqlitequeue--crash-restart)
- [Example 11 — `PathDirty` for tree-prefix invalidation](#example-11--pathdirty-for-tree-prefix-invalidation)
- [Cargo features](#cargo-features)
- [Phase E placeholder (reconciliation / cascade-delete)](#phase-e-placeholder-reconciliation--cascade-delete)
- [Where to look in the code](#where-to-look-in-the-code)

---

## Mental model in one screen

```
   seed values ─┐
                ▼
            ┌───────┐                ┌────────────┐
            │ Queue │ ──pull──►   render(c)  ─►  flatten(node) ── enqueue children
            └───────┘                  ▲              │
                ▲                      │              │
                │                      │ same pc?     │
                └──── Wake ────────────┴── Yield ◄────┘
                       ▲                              │ next pc?
                       │                              ▼
                  ┌─────────┐                    Suspense
                  │ EventBus │ ◄────── DomainDirty / KeyDirty / PathDirty
                  └─────────┘                  ▲
                       ▲                       │
                       │                       │
                MutationStore put          QueryCache
                                           MemoCache
```

- **Render is description, not execution.** A `Component::render` returns
  a `Node<N>` describing what to enqueue next. The driver does the
  enqueueing.
- **Pull, not push.** The queue holds state, the driver drains it. RAM
  is `O(in-flight pc count)`, not `O(stream length)`.
- **Wake = event = invalidation.** One `EventBus` carries all three
  signals; subscribers can be parker rows (Wake) or cache entries
  (BusListener).

---

## The seven core types

| Type | Job | Crate path |
|---|---|---|
| `trait Next` | Carrier marker; `content_hash() -> [u8; 32]` | `next.rs` |
| `NextKey([u8; 32])` | Content-derived row identity | `next_key.rs` |
| `Wake` | `Immediate` / `Tick` / `Key(NextKey)` | `wake.rs` |
| `enum Node<N>` | `Done` / `Emit` / `Many` / `Suspense` / `Yield` | `node.rs` |
| `trait Component` | `render(&self, ctx, c) -> Node<N>` | `component.rs` |
| `trait QueueBackend<N>` | `enqueue` / `pull_runnable` / `depth` | `queue.rs` |
| `EventBus` | `dispatch(Event)` + `subscribe_*` + listeners | `event_bus.rs` |

Plus three optional caches that ride on top:
`MutationStore<T>`, `MemoCache<N>`, `QueryCache<N>`.

---

## Example 1 — smallest possible pipe (i64 doubler)

The carrier doesn't have to be a cursor. Any `Send + Sync + 'static`
type with a content hash works. Primitive impls (`i64`, `u64`, `String`)
are provided.

```rust
use std::sync::{Arc, Mutex};
use effect_runtime::v2::{
    Component, DriveOpts, MemQueue, Node, PipeInstance,
    QueueBackend, RenderCtx, drive,
};

struct Double;
impl Component for Double {
    type Next = i64;
    fn render(&self, _: &RenderCtx, n: &i64) -> Node<i64> {
        Node::Emit(Arc::new(n * 2))
    }
}

struct Collect { sink: Arc<Mutex<Vec<i64>>> }
impl Component for Collect {
    type Next = i64;
    fn render(&self, _: &RenderCtx, n: &i64) -> Node<i64> {
        self.sink.lock().unwrap().push(*n);
        Node::Done
    }
}

let queue: Arc<dyn QueueBackend<i64>> = Arc::new(MemQueue::new());
let sink  = Arc::new(Mutex::new(Vec::new()));
let pipe  = PipeInstance::new(vec![
    Arc::new(Double) as Arc<dyn Component<Next = i64>>,
    Arc::new(Double),
    Arc::new(Collect { sink: sink.clone() }),
]);

drive(&pipe, queue, vec![Arc::new(3i64), Arc::new(5i64)], DriveOpts::default());

assert_eq!(*sink.lock().unwrap(), vec![12, 20]);
```

- Three components, one pipe.
- Two seed values flow through `Double → Double → Collect`.
- `Node::Emit(Arc::new(...))` is the "produce one downstream value" return.
- `Node::Done` is the terminal "consume, emit nothing".

---

## Example 2 — a real carrier (`LabCursor`) with `content_hash`

A useful carrier needs identity. `Next::content_hash` returns a stable
`[u8; 32]` that is the basis for `NextKey` and all the caches.

```rust
use effect_runtime::v2::Next;

#[derive(Clone)]
pub struct LabCursor {
    pub terms: Vec<(String, String)>,  // sorted, like sprefa::Cursor
}

impl Next for LabCursor {
    fn content_hash(&self) -> [u8; 32] {
        let mut h = blake3::Hasher::new();
        for (n, v) in &self.terms {
            h.update(&(n.len() as u32).to_le_bytes());
            h.update(n.as_bytes());
            h.update(&(v.len() as u32).to_le_bytes());
            h.update(v.as_bytes());
        }
        *h.finalize().as_bytes()
    }
}
```

Rules:
- Deterministic: same value, same hash, across runs and processes.
- Order-stable: if your carrier has internal sets/maps, sort first.
- Cheap: blake3 over a stable encoding is the canonical choice.

For sqlite-backed pipes the carrier also needs `Codec` (encode/decode
to bytes). See Example 10.

---

## Example 3 — fan-out via `Node::Many`

A component can return multiple downstream values in one render. They
flow concurrently — no implicit ordering between siblings.

```rust
struct FanOut { n: usize }
impl Component for FanOut {
    type Next = LabCursor;
    fn render(&self, _: &RenderCtx, c: &LabCursor) -> Node<LabCursor> {
        Node::Many((0..self.n).map(|i| {
            let mut copy = c.clone();
            copy.set(":fan_idx", i.to_string());
            Node::Emit(Arc::new(copy))
        }).collect())
    }
}
```

`Node::Many(vec![Emit, Emit, Emit])` enqueues three children, each at
`pc+1`, each `Wake::Immediate`.

---

## Example 4 — `Suspense` parks the row at `pc+1`

The first pause primitive. The component returns a value plus a wake
condition. The row goes to the parker bucket, the driver moves on. When
the wake fires (next section), the parker row becomes runnable at
`pc+1` (the NEXT component) with the parked value.

```rust
use effect_runtime::v2::{EventBus, Wake};

struct ParkOnKey { key: NextKey }
impl Component for ParkOnKey {
    type Next = LabCursor;
    fn render(&self, _: &RenderCtx, c: &LabCursor) -> Node<LabCursor> {
        Node::Suspense {
            value: Arc::new(c.clone()),
            wake:  Wake::Key(self.key),
        }
    }
}
```

`Suspense` advances `pc`. The same component does NOT re-render the
same value — see [Example 6](#example-6--yield-re-renders-the-same-component)
for the alternative.

---

## Example 5 — bus dispatch from a background thread (no async runtime)

`EventBus::dispatch(KeyDirty(k))` makes the parked row runnable on the
next `drive` iteration. The dispatch can come from any thread; v2 has
no built-in scheduler.

```rust
use std::time::Duration;
use effect_runtime::v2::{Event, EventBus};

let bus = Arc::new(EventBus::new());
let key = bus.fresh_key();

let pipe = PipeInstance::new(vec![
    Arc::new(ParkOnKey { key }) as Arc<dyn Component<Next = LabCursor>>,
    Arc::new(Collector { sink: sink.clone() }),
]);
let opts = DriveOpts::default().with_bus(bus.clone());

// First drive: parks.
drive(&pipe, queue.clone(), vec![lc(":raw", "alpha")], opts.clone());
assert_eq!(queue.depth(), 1);

// Off-thread, no executor.
let bus2 = bus.clone();
std::thread::spawn(move || {
    std::thread::sleep(Duration::from_millis(5));
    bus2.dispatch(Event::KeyDirty(key));
}).join().unwrap();

// Second drive: drains.
drive(&pipe, queue.clone(), Vec::new(), opts);
assert_eq!(sink.lock().unwrap().len(), 1);
```

---

## Example 6 — `Yield` re-renders the SAME component

`Suspense` advances `pc`. `Yield` parks at the same `pc` so when the
wake fires, the SAME component renders again with the same input. This
is the react-query state machine: first call sees `Idle`, transitions
to `Pending`, returns `Yield`. Second call (after the wake) sees
`Success(data)` and returns `Emit(data)`.

```rust
fn render(&self, _: &RenderCtx, c: &N) -> Node<N> {
    let key = self.key_for(&c.content_hash());
    match self.cache.status(key) {
        QueryStatus::Success(data) => Node::Emit(data),
        QueryStatus::Pending       => Node::Yield {
            value: Arc::new(c.clone()),
            wake:  Wake::Key(key),
        },
        QueryStatus::Idle => {
            // ... spawn queryFn ...
            Node::Yield { value: Arc::new(c.clone()), wake: Wake::Key(key) }
        }
        QueryStatus::Error(_) => Node::Done,
    }
}
```

Use `Yield` when the SAME component decides what to do based on
external state (cache, response). Use `Suspense` when the NEXT
component consumes the resolved value.

---

## Example 7 — saga-style effects: `EffectDispatch` + `Spawner`

The boilerplate of "spawn a thread, do work, write the result, dispatch
KeyDirty" collapses into one call.

```rust
use effect_runtime::v2::{
    EffectDispatch, MutationStore, ThreadSpawner, TokioSpawner,
};

let bus     = Arc::new(EventBus::new());
let store   = Arc::new(MutationStore::<LabCursor>::new());
let fx      = Arc::new(EffectDispatch::new(
    bus.clone(),
    store.clone(),
    Arc::new(ThreadSpawner),     // or TokioSpawner
));

struct DispatchUppercase { fx: Arc<EffectDispatch<LabCursor>> }
impl Component for DispatchUppercase {
    type Next = LabCursor;
    fn render(&self, _: &RenderCtx, c: &LabCursor) -> Node<LabCursor> {
        let key  = NextKey(c.content_hash());
        let raw  = c.get(":raw").unwrap_or("").to_string();
        let seed = c.clone();
        self.fx.dispatch(key, move || {
            let mut out = seed;
            out.set(":upper", raw.to_uppercase());
            out
        });
        Node::Suspense { value: Arc::new(c.clone()), wake: Wake::Key(key) }
    }
}
```

The next component reads the result via `store.take(key)`.

`Spawner` is a trait — `ThreadSpawner` for the no-runtime path,
`TokioSpawner` to share a `tokio::spawn_blocking` pool with an existing
runtime, your own impl for anything else.

---

## Example 8 — `useMemo` (`Memoize<C>`) with domain invalidation

Wrap a component, cache its `Node<N>` output by
`(ident, input.content_hash())`. Tag with one or more domains so
`DomainDirty(d)` drops every entry tagged with `d`.

```rust
use effect_runtime::v2::{
    attach_cache_to_bus, MemoCache, Memoize,
};

let cache = Arc::new(MemoCache::<LabCursor>::new());
attach_cache_to_bus(cache.clone(), &bus);

let memoized = Memoize::new(
    inner_component,
    "trim_clean",
    cache.clone(),
).with_domain("fs");

// First render: miss, runs inner, caches. Second render with same
// content_hash: hit, no re-execution.
// bus.dispatch(Event::DomainDirty("fs"));  ⇒ cache cleared.
```

The cache is a `BusListener`; the dispatch path is `bus.dispatch(...)`,
the cache reacts.

---

## Example 9 — `useQuery` (`Query<N, F>`) with `invalidateQueries`

Like `Memoize`, but the wrapped function runs off-thread and the cache
holds `QueryStatus { Idle, Pending, Success, Error }`. `EffectKey =
blake3(ident, input.content_hash())`.

```rust
use effect_runtime::v2::{
    attach_query_cache_to_bus, Query, QueryCache, QueryFn, ThreadSpawner,
};

struct ListReposQueryFn;
impl QueryFn<LabCursor> for ListReposQueryFn {
    fn ident(&self) -> &'static str { "list_repos" }
    fn run(&self, input: &LabCursor) -> LabCursor {
        let mut out = input.clone();
        out.set(":repos", expensive_io());
        out
    }
}

let cache = Arc::new(QueryCache::<LabCursor>::new());
attach_query_cache_to_bus(cache.clone(), &bus);

let q = Query::new(
    ListReposQueryFn,
    cache.clone(),
    bus.clone(),
    Arc::new(ThreadSpawner),
).with_domain("repos");

// First render: Idle → spawn → Yield. Wait. Re-render: Success → Emit.
// bus.dispatch(Event::DomainDirty("repos"));  ⇒ cache cleared, next
// render re-fires the queryFn.
```

Same Component code regardless of whether the queryFn is sync, async,
local, or networked. The Spawner is the seam.

---

## Example 10 — `SqliteQueue` + crash-restart

Same `QueueBackend<N>` trait, durable storage. Requires `Codec` on the
carrier so the runtime can serialize rows.

```rust
use effect_runtime::v2::{Codec, SqliteMutationStore, SqliteQueue};

impl Codec for LabCursor {
    fn encode(&self) -> Vec<u8> { /* length-prefixed terms */ }
    fn decode(bytes: &[u8]) -> Self { /* inverse */ }
}

// Process 1: park, persist mutation result, simulate crash.
{
    let conn = Arc::new(StdMutex::new(rusqlite::Connection::open(&path)?));
    let queue: Arc<dyn QueueBackend<LabCursor>> =
        Arc::new(SqliteQueue::open(conn.clone()));
    let store = Arc::new(SqliteMutationStore::<LabCursor>::open(conn.clone()));

    drive(&pipe, queue.clone(), vec![input.clone()], opts);
    assert_eq!(queue.depth(), 1);
    store.put(deterministic_key, Arc::new(result));
    // drop everything ⇒ process exit
}

// Process 2: reopen file, fresh bus, fresh driver, redrive.
{
    let conn = Arc::new(StdMutex::new(rusqlite::Connection::open(&path)?));
    let queue: Arc<dyn QueueBackend<LabCursor>> =
        Arc::new(SqliteQueue::open(conn.clone()));
    let store = Arc::new(SqliteMutationStore::<LabCursor>::open(conn.clone()));
    let bus   = Arc::new(EventBus::new());

    bus.dispatch(Event::KeyDirty(deterministic_key));
    drive(&pipe, queue.clone(), Vec::new(),
          DriveOpts::default().with_bus(bus));
    // sink output identical to never-crashed run.
}
```

The `Connection` is shared (`Arc<Mutex<Connection>>`) so the consumer
crate can run queue mutations and its own relational writes in the
same transaction.

---

## Example 11 — `PathDirty` for tree-prefix invalidation

Every `QueueRow` carries `path: Vec<u32>` populated as
`parent.path + [batch_idx]`. `subscribe_path(prefix, key)` registers a
key against a prefix; `dispatch(PathDirty(prefix))` wakes every key
whose registered path starts with that prefix.

```rust
let key_a = bus.fresh_key();
let key_b = bus.fresh_key();
let key_c = bus.fresh_key();
bus.subscribe_path(vec![1, 2, 5], key_a);
bus.subscribe_path(vec![1, 2, 9], key_b);
bus.subscribe_path(vec![7, 0],    key_c);  // not a descendant

bus.dispatch(Event::PathDirty(vec![1, 2]));
assert!( bus.is_ready(key_a));
assert!( bus.is_ready(key_b));
assert!(!bus.is_ready(key_c));
```

Sets up the prefix-LIKE indexed cascade-delete that Phase E will use.

---

## Cargo features

```toml
[features]
default = ["sqlite"]
sqlite  = ["dep:rusqlite"]
rx      = ["dep:rxrust"]
```

- `sqlite` (default-on): pulls `rusqlite 0.32 / bundled`. Disable with
  `default-features = false` if the consumer crate already has a
  different rusqlite version. With sqlite off, `MemQueue` +
  `MutationStore` are still available; only `SqliteQueue` and
  `SqliteMutationStore` are gated out.
- `rx` (off by default): rxRust integration in the parent crate, not
  used by v2.

---

## Phase E placeholder (reconciliation / cascade-delete)

Not implemented yet. When a parent re-renders with a different child
set (Memoize cache invalidation propagation, Query Success → Idle), the
prior children become orphans in the queue. Phase E adds:

- `QueueBackend::cascade_delete(parent_id) -> u64`
- per-parent prior-children index (for multiset diff)
- driver hook: diff prior vs new children, cascade-delete missing,
  enqueue new

TODO comments mark the five sites where Phase E hooks land:
`queue.rs`, `flatten.rs`, `driver.rs`, `memoize.rs`, `query.rs`.

---

## Where to look in the code

| File | Reads in |
|---|---|
| `next.rs`              | <60 LoC. Trait + primitive impls. |
| `next_key.rs`          | <50 LoC. Compose helper. |
| `wake.rs`              | <20 LoC. Three variants. |
| `node.rs`              | <60 LoC. Five variants + manual Clone. |
| `component.rs`         | <40 LoC. Trait + RenderCtx + DynComponent alias. |
| `queue.rs`             | ~80 LoC. Trait + QueueRow + ReadyKeys. |
| `mem_queue.rs`         | ~110 LoC. Three buckets + one mutex. |
| `flatten.rs`           | ~100 LoC. Pure Node → `Vec<QueueRow>`. |
| `driver.rs`            | ~140 LoC. Pull / render / flatten / enqueue loop. |
| `event_bus.rs`         | ~140 LoC. Dispatch + listeners + path/domain subs. |
| `mutation_store.rs`    | <60 LoC. RAM Arc<T>-by-NextKey. |
| `sqlite_queue.rs`      | ~210 LoC. Schema + impls. |
| `sqlite_mutation_store.rs` | ~100 LoC. Schema + impls. |
| `effect_dispatch.rs`   | ~90 LoC. Spawner trait + ThreadSpawner / TokioSpawner. |
| `memoize.rs`           | ~150 LoC. Cache + BusListener + Memoize HOC. |
| `query.rs`             | ~190 LoC. Status + cache + Component impl. |
| `tests.rs`             | ~1100 LoC. 19 test harness for everything above. |
