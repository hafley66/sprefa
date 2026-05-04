# `effect_runtime::v2` — pull-based react-shaped runtime

> **Park-as-row redesign (current).** Wake subscriptions live on rows
> in the queue, not in the bus. `Wake::Key { domain, key }` carries
> both fields; producers call `queue.dispatch_park(domain,
> Some(key))` to flip every matching parked row to `Wake::Immediate`
> via one indexed UPDATE. `EventBus` retains only cache fan-out
> (`Event::Dirty { domain, key }`); the variants `KeyDirty`,
> `PathDirty`, `DomainDirty`, plus `subscribe_path` /
> `subscribe_domain` / `is_ready` / `ready_count` no longer exist.
> Some example snippets below still reference the old API for
> historical context. The live API shape is in `tests.rs` +
> `event_bus.rs` + `queue.rs`.

A queue-backed runtime for **streaming, content-addressed, durable** value pipelines.
Generic over the carrier (the value flowing through). Components return
descriptions of what should happen next; the driver executes. Park
subscriptions live in the queue (indexed by `(domain, key)`); cache
invalidation flows through `EventBus`. Sqlite-backed for redux-persist
semantics across process restart.

The vocabulary deliberately matches React / Redux / react-query / redux-saga / RxJS
since the substrate maps cleanly onto each idiom.

---

## Table of contents

- [Glossary](#glossary)
- [Mental model in one screen](#mental-model-in-one-screen)
- [The seven core types](#the-seven-core-types)
- [Example 1 — smallest possible pipe (i64 doubler)](#example-1--smallest-possible-pipe-i64-doubler)
- [Example 2 — a real carrier (`LabCursor`) with `content_hash`](#example-2--a-real-carrier-labcursor-with-content_hash)
- [Example 3 — fan-out via `Node::Many`](#example-3--fan-out-via-nodemany)
- [Example 4 — `Yield` parks the row at the same depth](#example-4--yield-parks-the-row-at-the-same-depth)
- [Example 5 — bus dispatch from a background thread (no async runtime)](#example-5--bus-dispatch-from-a-background-thread-no-async-runtime)
- [Example 6 — react-query state machine via `Yield`](#example-6--react-query-state-machine-via-yield)
- [Example 7 — saga-style effects: `EffectDispatch` + `Spawner`](#example-7--saga-style-effects-effectdispatch--spawner)
- [Example 8 — `useMemo` (`Memoize<C>`) with domain invalidation](#example-8--usememo-memoizec-with-domain-invalidation)
- [Example 9 — `useQuery` (`Query<N, F>`) with `invalidateQueries`](#example-9--usequery-queryn-f-with-invalidatequeries)
- [Example 10 — `SqliteQueue` + crash-restart](#example-10--sqlitequeue--crash-restart)
- [`HybridQueue` — write-back cache (mem hot, sqlite cold)](#hybridqueue--write-back-cache-mem-hot-sqlite-cold)
- [Example 11 — `PathDirty` for tree-prefix invalidation](#example-11--pathdirty-for-tree-prefix-invalidation)
- [Component override tiers (render / render_batch / dispatch)](#component-override-tiers-render--render_batch--dispatch)
- [Cargo features](#cargo-features)
- [Phase E placeholder (reconciliation / cascade-delete)](#phase-e-placeholder-reconciliation--cascade-delete)
- [Where to look in the code](#where-to-look-in-the-code)

---

## Glossary

A few terms appear throughout. Worth pinning before the examples.

- **carrier** — the value type flowing through the pipe (`i64`, `LabCursor`, `Cursor`, …). Every carrier impls `Next`. The substrate is generic over it.
- **pipe** — `Vec<Component>`, the linear sequence of stages a value walks through.
- **depth** — index into the pipe. A `QueueRow { depth: 2, ... }` means "this value should next be rendered by `pipe.components[2]`." Increments on `Emit` / `Many`. Stays put on `Yield`. Hits `pipe.len()` and the row terminates.
- **path** — `Vec<u32>` per row, the sibling-index trail from the pipe root: `parent.path + [batch_idx]`. Two siblings have paths that differ in the last segment. Powers `PathDirty` prefix invalidation and (in Phase F) sqlite prefix indices.
- **wake** — the condition that makes a parked row runnable: `Immediate`, `Tick { past_tick }`, or `Key(NextKey)`.
- **bus** — the `EventBus`. One `dispatch(Event)` call serves both wake (parker rows go runnable) and invalidation (cache listeners drop entries).
- **queue len vs row depth** — `queue.len()` is the count of resident rows (queue size). `row.depth` is the position-in-pipe of one row. Different concepts, both happen to be unsigned integers.

---

## Mental model in one screen

```
   seed values ─┐
                ▼
            ┌───────┐                ┌────────────┐
            │ Queue │ ──pull──►   render(c)  ─►  flatten(node) ── enqueue children
            └───────┘                  ▲              │
                ▲                      │              │
                │                      │ Yield: park at same depth
                └──── Wake ────────────┴───────────────┘
                       ▲
                       │
                  ┌─────────┐
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
  is `O(in-flight depth count)`, not `O(stream length)`.
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
| `enum Node<N>` | `Done` / `Emit` / `Many` / `Yield` | `node.rs` |
| `trait Component` | `render(&self, ctx, c) -> Node<N>` | `component.rs` |
| `trait QueueBackend<N>` | `enqueue` / `pull_runnable` / `len` | `queue.rs` |
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
`depth+1`, each `Wake::Immediate`.

---

## Example 4 — `Yield` parks the row at the same depth

The pause primitive. The component returns a value plus a wake
condition. The row goes to the parker bucket, the driver moves on. When
the wake fires (next section), the parker row becomes runnable at the
SAME depth — the same component renders again with the same input.

```rust
use effect_runtime::v2::{EventBus, Wake};

struct ParkOnKey { key: NextKey, fired: AtomicBool }
impl Component for ParkOnKey {
    type Next = LabCursor;
    fn render(&self, _: &RenderCtx, c: &LabCursor) -> Node<LabCursor> {
        if self.fired.swap(true, Ordering::SeqCst) {
            // Wake fired: emit downstream.
            Node::Emit(Arc::new(c.clone()))
        } else {
            Node::Yield {
                value: Arc::new(c.clone()),
                wake:  Wake::Key(self.key),
            }
        }
    }
}
```

`Yield` parks at the parker's own depth. Render must be idempotent
against the input — the component sees its input twice (first call
yields, second call after wake decides what to do). See
[Example 6](#example-6--react-query-state-machine-via-yield)
for the canonical state-machine shape.

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
    Arc::new(ParkOnKey::new(key)) as Arc<dyn Component<Next = LabCursor>>,
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

## Example 6 — react-query state machine via `Yield`

`Yield` parks at the same `depth`, so when the wake fires the SAME
component renders again with the same input. The react-query state
machine: first call sees `Idle`, transitions to `Pending`, returns
`Yield`. Second call (after the wake) sees `Success(data)` and returns
`Emit(data)`.

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

`Yield` is the sole park primitive. The same component decides what to
do based on external state (cache, response, store).

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

struct DispatchUppercase {
    fx:    Arc<EffectDispatch<LabCursor>>,
    store: Arc<MutationStore<LabCursor>>,
}
impl Component for DispatchUppercase {
    type Next = LabCursor;
    fn render(&self, _: &RenderCtx, c: &LabCursor) -> Node<LabCursor> {
        let key = NextKey(c.content_hash());
        // Re-render path: result has landed, take and emit.
        if let Some(r) = self.store.take(key) {
            return Node::Emit(r);
        }
        // First-render path: dispatch and yield.
        let raw  = c.get(":raw").unwrap_or("").to_string();
        let seed = c.clone();
        self.fx.dispatch(key, move || {
            let mut out = seed;
            out.set(":upper", raw.to_uppercase());
            out
        });
        Node::Yield { value: Arc::new(c.clone()), wake: Wake::Key(key) }
    }
}
```

The same component reads the result via `store.take(key)` on re-render
after wake.

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

## `HybridQueue` — write-back cache (mem hot, sqlite cold)

`HybridQueue<N>` layers `MemQueue<N>` over `SqliteQueue<N>` so a
long-running server holding 10k file watches does not pay a sqlite
fsync per park. Fresh parks live in RAM. Aged parks flush to sqlite
in batches via `tick_flush()`. The crash window equals
`park_flush_interval`; that knob is the explicit durability dial.

```rust
use std::time::Duration;
use effect_runtime::v2::{HybridCfg, HybridQueue, QueueBackend};

let cfg = HybridCfg {
    park_flush_interval: Duration::from_millis(100), // ZERO = write-through
    park_mem_cap:        10_000,                     // RSS backstop
    park_flush_batch:    256,                        // rows per tick
};
let q = HybridQueue::<MyCarrier>::open_in_memory(cfg).unwrap();
```

Behavior:

- `enqueue` always lands in hot. If hot park count exceeds
  `park_mem_cap`, the backstop force-flushes oldest parks to cold.
- `pull_runnable` and `pull_runnable_batch` fall through hot → cold.
- `dispatch_park` fans both tiers; whichever holds the row promotes.
- `tick_flush()` evicts `Wake::Key` rows aged past
  `park_flush_interval` from hot, bulk-inserts into cold in one
  transaction. `Wake::Immediate` and `Wake::Tick` rows stay hot.

Tuning: `park_flush_interval = Duration::ZERO` writes every park
through on the next `tick_flush`. `Duration::MAX` keeps everything in
RAM until `park_mem_cap` triggers eviction.

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

## Component override tiers (render / render_batch / dispatch)

The `Component` trait exposes three layered entry points. Implement
the one that fits the work; the rest fall through via defaults. None
is mandatory — a Component that overrides nothing is a no-op (drops
every input).

```text
                                          ┌─ default = drop input
  tier 1  render(&self, ctx, &N) ────────┤
                                          └─ override = per-row pure transform

                                          ┌─ default = loop render
  tier 2  render_batch(&self, ctx, &[&N])┤
                                          └─ override = batch-shaped work
                                                       (rayon, SIMD, sqlite IN(...))

                                          ┌─ default = render_batch + splice
  tier 3  dispatch(&self, ctx, rows,    ─┤
                   queue, bus)            └─ override = full substrate control
                                                       (mergeMap, switchMap,
                                                        parker enqueue, debounce,
                                                        Spawner handoff)
```

Defaults flow inner → outer (`dispatch` → `render_batch` → `render`).
The terminal default for `render` is `Node::Done`, so the chain
cannot recurse.

### Tier 1 — `render`

```rust
struct Trim { from: String, to: String }
impl Component for Trim {
    type Next = LabCursor;
    fn render(&self, _: &RenderCtx, c: &LabCursor) -> Node<LabCursor> {
        let raw = c.get(&self.from).unwrap_or("").to_string();
        let mut next = c.clone();
        next.set(&self.to, raw.trim());
        Node::Emit(Arc::new(next))
    }
}
```

### Tier 2 — `render_batch` with rayon (the rxjs `mergeMap` analog)

```rust
use effect_runtime::v2::{par_render, Component, Node, RenderCtx};

struct ParAstNm { /* compiled ast-grep pattern */ }
impl Component for ParAstNm {
    type Next = Cursor;
    fn render_batch(&self, _: &RenderCtx, batch: &[&Cursor]) -> Vec<Node<Cursor>> {
        // par_render maps over rayon's pool, results in input order.
        par_render(batch, |c| {
            let matches = self.run_pattern(c);
            Node::Many(matches.into_iter().map(|m| {
                let mut next = c.clone();
                next.set(":match", m);
                Node::Emit(Arc::new(next))
            }).collect())
        })
    }
}
```

`render_batch` sees up to `DriveOpts::batch_cap` rows (default 256)
that share `(pipe_hash, depth)`. The batch shape lets the override
amortize fixed work (compiled patterns, SIMD setup) across the slice.

### Tier 3 — `dispatch` for substrate-aware ops

`dispatch` owns the queue interaction. Use it to dispatch bus events,
enqueue children with custom `Wake`, hand work to a `Spawner`, or
implement reactive operators that don't fit a per-row transform.

```rust
use effect_runtime::v2::{
    splice_into, Component, Event, EventBus, Node, QueueBackend,
    QueueRow, RenderCtx, Wake,
};

/// switchMap-style: every new emission cancels the prior parker.
struct SwitchMap { bus: Arc<EventBus>, prev: Mutex<Option<NextKey>> }
impl Component for SwitchMap {
    type Next = LabCursor;
    fn dispatch(
        &self,
        ctx:   &RenderCtx,
        rows:  &[QueueRow<LabCursor>],
        queue: &dyn QueueBackend<LabCursor>,
        bus:   &EventBus,
    ) {
        for row in rows {
            let new_key = self.bus.fresh_key();
            if let Some(prev) = self.prev.lock().unwrap().replace(new_key) {
                bus.dispatch(Event::KeyDirty(prev));   // cancel prior
            }
            let parked = Node::Yield {
                value: row.value.clone(),
                wake:  Wake::Key(new_key),
            };
            splice_into(row, parked, ctx.depth, ctx.drive_tick, queue);
        }
    }
}
```

| rxjs operator | What the `dispatch` override does |
|---|---|
| `mergeMap(N)`              | rayon-spawn N renders concurrently; splice as they complete |
| `switchMap`                | dispatch `KeyDirty(prev)`; enqueue child with fresh `Wake::Key` |
| `concatMap`                | enqueue child with `Wake::Key(prev_completion)`; dispatch on completion |
| `debounceTime(ms)`         | `Wake::Tick { past_tick: now + ms_in_ticks }` |
| `throttleTime(ms)`         | drop input if `last_emit_tick + ms > now_tick` |
| `distinctUntilChanged`     | compare `c.content_hash()` to last seen, drop dupes |
| `bufferTime(ms)`           | self-loop `Wake::Tick`, accumulate batch in a Mutex, flush on tick |
| sprefa `next?(:event)`     | enqueue `Wake::Key(blake3(":event"))`, source op fires `bus.dispatch(KeyDirty(same))` |
| sprefa `Sh / FactWrite`    | hand to `Spawner`; on completion, `mutation_store.put` + `KeyDirty` |
| sprefa OpCache (Layer-3)   | check `MemoCache` before splice; cache hit short-circuits |

### Driver knob

`DriveOpts::with_batch_cap(n)` caps how many rows the driver pulls
per dispatch. Default 256 (matches v3's `DEFAULT_PIPE_CONCURRENCY`).
`Some(1)` forces per-row delivery.

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
| `node.rs`              | <60 LoC. Four variants + manual Clone. |
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
