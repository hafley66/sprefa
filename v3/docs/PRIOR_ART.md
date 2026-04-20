# Prior art scan: typed effect dispatcher runtimes

Scope: Rust + adjacent. Specifically for a library that looks like
`ctx.put(E).await -> E::Response` with per-kind-registry dispatch,
per-kind topology (sync / rayon / bounded-mpsc / batched), per-kind
backpressure, monomorphized handlers behind one `dyn` cell, op-author
writes `EffectKind + Batcher<E>` in one file.

Ordered by how closely the project matches the shape. Survey dated
2026-04-20.

---

## 1. `batched-fn` (epwalsh)

crates.io: `batched-fn`, ~0.2.x, maintained. Built for ML inference,
>1k downloads.

**Dispatcher shape.** A macro wraps a function body; invocation
enqueues `(input, oneshot_tx)` onto a flume channel; a single handler
thread drains up to `max_batch_size` with a `max_delay_ms` trailing
time window, runs the batch, fans responses back through oneshots.
One handler per macro site; compile-time typing via closure; one queue
per decorated function.

**Right to copy.**
- Adaptive batch: `max_batch_size` + `max_delay` is the canonical
  drain-and-coalesce knob pair. Matches `BoundedBatched` exactly.
- `(value, oneshot_tx)` over mpsc is the minimal futureful RPC.
- Handler owns a per-handler context struct captured once, reused.

**Gaps.**
- No registry. Each decorated function is standalone; no `TypeId`
  lookup, no dynamic add/remove.
- One topology hardcoded. Cannot opt out per kind.
- No per-kind telemetry, no hot swap.

**Has `BoundedBatched`-like shape:** yes. Reference implementation.

---

## 2. `multistream-batch` (jpastuszek)

Pure batching algorithms: `TxBuffer`, `MultiBufStream`, multi-key
batching with per-key buckets and global flush conditions (size,
linger time, count).

**Right to copy.**
- Multi-keyed batching: separate keyed buckets flush independently.
  Closer to per-effect-kind model than single-queue batchers.
- Exposes batch policy as enum (`MaxDuration`, `MaxItems`,
  `CombinedSize`, …). Policy is data, not code — suggests the
  `Batcher` surface could be data-driven.

**Gaps.**
- No RPC shape (no response type). Pure fan-in batching.
- No registry, no dispatcher.

**Has `BoundedBatched`-like shape:** partial. Drain + windowed-trailing.

---

## 3. DataLoader family

`dataloader` (cksac), `async-graphql::dataloader`, Juniper's impl.
All port Facebook DataLoader (2016, JS).

**Dispatcher shape.** `trait BatchFn<K, V> { async fn load(&self,
keys: &[K]) -> HashMap<K, V> }`. Caller does `loader.load(key).await`.
Loader coalesces all `load` calls that arrive within the same tick
(scheduler yield) into one batch, dedups keys, dispatches once, fans
results back. One loader instance per `(BatchFn impl, K, V)` triple.

**Right to copy.**
- Applicative batching within a tick. Two independent `put(E1)` and
  `put(E2)` in the same pipeline stage fuse. Haxl's core insight
  landed here.
- Per-request scope via `loader_per_request` is the analog of
  per-pipeline-run ctx.
- Key deduplication in the batch is free: `HashSet<Key>` around drain.

**Gaps.**
- No registry pattern. User wires loaders manually into ctx.
- One topology (tick-bounded coalesce). No rayon, no
  bounded-queue + work-steal variants.
- Cache coupled into the loader, not separable.

**Has `BoundedBatched`-like shape:** yes, with tick semantics instead
of explicit time window.

---

## 4. `tower` (tower-rs)

`tower` / `tower-service`. Widely deployed, 1.x mature.

**Dispatcher shape.** `trait Service<Req> { type Response; type Error;
type Future; fn poll_ready; fn call }`. `Layer` composes
`Service -> Service`. Generic over request type; one service per type
parameter. `tower::buffer::Buffer` wraps a service behind a bounded
mpsc worker; `ConcurrencyLimit`, `RateLimit`, `Timeout`, `LoadShed`
are middlewares.

**Right to copy.**
- Typed `Service<Req>` with associated `Response` is the exact
  `EffectKind -> Response` shape.
- `Layer` stacking lets topology be assembled
  (`Batch -> Buffer -> Concurrency -> Inner`). Each knob is a crate.
- `poll_ready` gives backpressure before `call`. Prevents
  oversubscription without hidden queues.
- `tower::balance` gives per-service load-aware dispatch, closest to
  per-kind work-stealing.

**Gaps.**
- No `TypeId` registry; per-request-type dispatch is static. A central
  `ctx.put(erased) -> typed response` requires re-erasing the return.
- Each service is fixed-topology after construction. Hot-swap means
  rebuilding and `Arc`-storing atomically. That pattern is on users.
- No built-in coalescing/batching layer; `tower-batch-control` exists
  but is minimal.

**Has `BoundedBatched`-like shape:** partial via `tower::buffer`
(bounded queue) + manual batching layer.

---

## 5. Kameo (tqwewe)

Actor framework, actively maintained (2025-2026). Next to Xtra and
Ractor in the Rust actor landscape.

**Dispatcher shape.** `trait Actor` owns `type Mailbox`. Each message
type has its own `impl Message<MyMsg> for MyActor` with `type Reply`.
`ActorRef::ask(msg).await -> Reply`. Per-actor bounded/unbounded
mailbox, backpressure on send. Spawn policy pluggable (tokio task,
pool).

**Right to copy.**
- Per-message-type `Reply` associated type is the typed-response
  surface.
- Per-actor mailbox size → per-kind backpressure (if every
  `EffectKind` maps to one actor).
- Linking + supervision for graceful shutdown — applies directly to a
  handler registry.

**Gaps.**
- Actor = one mailbox shared by all message types that actor handles.
  A slow `MsgA` handler blocks `MsgB` in the same actor. Topology per
  `EffectKind` requires one actor per kind, at which point actor
  scaffolding is overhead.
- No batching primitive.
- No `TypeId` dispatcher; reference is the type.

Other Rust actors:
- Xtra: similar shape, multi-runtime, `Handler<M>` with `type Return`.
- Ractor: one message enum per actor; type-erases inside the enum.
  Further from goal.
- Actix: same per-message-type dispatch, heavier runtime.

**Has `BoundedBatched`-like shape:** no, not in any Rust actor
framework out of the box.

---

## 6. Salsa (salsa-rs)

Incremental query framework. rust-analyzer dogfoods it.

**Dispatcher shape.** `#[salsa::tracked] fn q(db: &dyn Db, k: K) -> V`.
Per-query storage strategy (`memoized`, `input`, `interned`,
`tracked`). Database is the registry; query function identity is the
key. Lookup is per-query-`TypeId`-equivalent through generated glue.

**Right to copy.**
- Per-query storage choice is the direct analog of per-kind topology.
  Salsa picks memoization strategy per query; the dispatcher picks
  concurrency strategy per effect.
- Single `&dyn Db` cell hides heterogeneous query tables behind one
  pointer — mirrors the "one dyn cell" monomorphization goal.
- Revision-based invalidation could map to effect cache.

**Gaps.**
- Everything is synchronous memoization + dependency tracking. No
  async dispatch, no batching across queries.
- Topology is storage, not concurrency.
- No backpressure concept.

**Has `BoundedBatched`-like shape:** no.

---

## 7. Bevy events / commands / observers

Bevy ECS, actively maintained.

**Dispatcher shape.** `Events<T>` is per-type double buffer;
`EventWriter<T>` appends, `EventReader<T>` pulls. `Commands` queues
typed mutations replayed at sync points. Observers (newer) dispatch
per-event-type to registered systems. All lookups by `TypeId` through
`Resource`/`Component` storage.

**Right to copy.**
- Per-type storage keyed by `TypeId` is Bevy's spine. Shows the
  monomorphization-per-kind + one-table pattern at scale.
- Command buffer + flush-at-sync-point is a deferred-effect pattern,
  almost identical to sprefa's `MutationEffect`.
- Observers give typed callbacks without centralized match.

**Gaps.**
- No RPC shape — events are fire-and-forget, no `Response`.
- No backpressure — Vec grows unboundedly per frame.
- No batching primitive. No work-stealing per-event-type.

**Has `BoundedBatched`-like shape:** no.

---

## 8. `effing-mad` / `reffect` / `eff`

`effing-mad` (rosefromthedead), `reffect` (js2xxx), `eff` (0.1,
dormant since 2019).

**Dispatcher shape.** Coroutine-based algebraic effects. Effectful fn
yields typed effect values; a `handler!` match performs the action
and resumes. Handlers are lexically scoped via `handle(body,
handler)`, not registered.

**Right to copy.**
- Typed yield/resume with per-effect response is the cleanest
  surface-level model of "put(E) -> Response".

**Gaps.**
- No runtime dispatcher at all. Handler is a closure around the call
  site. Zero infra for backpressure, batching, registry, topology-
  per-kind.
- Stability: all three sit on unstable compiler features; unsuitable
  as runtime foundation.

**Has `BoundedBatched`-like shape:** no.

---

## 9. Non-Rust reference (brief)

**Haxl (Haskell 2014).**
```haskell
class DataSource req where
  fetch :: State req -> Flags -> Environment
        -> [BlockedFetch req] -> PerformFetch
```
Per-req-type `DataSource` instance is the registry entry;
`PerformFetch` variants are `SyncFetch`, `AsyncFetch`,
`BackgroundFetch` — per-kind topology, in 2014. **Closest structural
match to the goal of any library surveyed.** Applicative Do coalesces
independent fetches. Core idea to port: `fetch :: [Request] -> IO
[Response]` is batcher-friendly by construction because the arg list
is plural.

**ZIO (Scala).** `ZIO[R, E, A]`. Executor is per-fiber, lockable via
`onExecutor`. Blocking pool separate from compute pool. Per-effect
executor, not per-kind dispatcher — granularity is the fiber, not the
effect tag. No typed registry keyed by effect kind.

**Cats Effect.** Similar to ZIO via `IO`. Executor plumbing is
global/lexical, not per-effect-type.

**Koka.** Static evidence-passing resolves handler at compile time.
Constant-time dispatch, inlineable. No runtime registry; handlers are
scoped.

**OCaml 5.** Unchecked dynamic effects. Nearest enclosing
`try..effect` handler. Runtime lookup is a stack walk, not a table.
No batching, no backpressure.

**Effekt.** Type-directed selective CPS. Closer to Koka than to a
runtime.

**Java `CompletableFuture` + custom executors.** Per-stage executor
via `thenApplyAsync(fn, executor)`. No typed registry. Vertx/Reactor
patterns layer RxJava on top; typed is via class of event, not
TypeId.

**Akka-typed (Scala).** `Behavior[T]` per actor, typed messages. Same
mailbox-shared-by-type limitation as Rust actors.

---

## 10. Naming conventions in use

Terms that exist in this territory:
- "DataSource" (Haxl), "DataLoader" (JS/graphql), "Loader" (Rust
  dataloader)
- "Service" (tower), "Handler" (actix, axum, tower), "Layer"
- "Effect", "Handler", "Perform", "Resume" (algebraic effects camp)
- "Batcher", "Coalescer", "Collector" (batching crates)
- "Dispatcher", "Router", "Registry" (generic)
- "Kind", "Tag", "Slot" (sprefa and Bevy idiom)

Not yet taken on crates.io as single-word packages (verify at publish
time): `effect-dispatch`, `effect-runtime`, `typed-effects`,
`effect-kinds`, `effect-bus`.

Taken: `effects`, `eff`, `effing-mad`, `reffect`, `tower`,
`batch-channel`, `batched-fn`, `multistream-batch`, `salsa`, `kameo`,
`xtra`.

The phrase "effect dispatcher" is underused. "Effect runtime" collides
with FRP / algebraic-effects crates. "Effect bus" carries Kafka/CQRS
connotations.

---

## Summary matrix

| Project | Typed Resp | TypeId Registry | Per-kind Topology | Per-kind Backpressure | Adaptive Batch | Hot Swap |
|---|---|---|---|---|---|---|
| batched-fn | Y (closure) | N | fixed | Y (1 kind) | Y | N |
| multistream-batch | N | N (keyed) | N | Y | Y | N |
| dataloader-rs | Y | N | fixed (tick) | Y | Y (tick) | N |
| tower | Y | N (per-Req) | assembled via Layer | Y (Buffer) | N (third-party) | via ArcSwap |
| kameo / xtra | Y | N | one topology per actor | Y (mailbox) | N | N |
| actix | Y | N | fixed | Y | N | N |
| ractor | partial (enum) | N | fixed | Y | N | N |
| salsa | Y (sync) | TypeId-ish | storage-per-query | N | N | N |
| bevy events | fire-and-forget | Y | N | N | N | N |
| effing-mad / reffect | Y | lexical | N | N | N | N |
| Haxl (Haskell) | Y | per-DataSource | `PerformFetch` variants | Y | Y | N |
| ZIO | Y | N | per-fiber executor | partial | N | N |

## Conclusion: gap the library fills

Closest matches:
- **Haxl's `DataSource` + `PerformFetch`** for the typed-registry-
  with-per-kind-topology shape.
- **tower's `Service<Req>` + `Layer`** for the Rust-native typed-RPC
  and backpressure primitives.
- **`batched-fn`** for the drain-and-coalesce `BoundedBatched` piece.

The combined design (TypeId registry + per-kind `PerformFetch`-analog
+ tower-style layer composition + `batched-fn`-style windowed drain
+ core telemetry) **does not exist as a single crate in Rust.**

That gap is what the effect-runtime crate would fill. The structural
moves (per-kind topology, typed responses, one-file op authoring, one
dyn cell) each appear somewhere above; their union does not.

---

## Sources

- [batch-channel (chadaustin)](https://github.com/chadaustin/batch-channel)
- [batched-fn (epwalsh)](https://github.com/epwalsh/batched-fn)
- [multistream-batch](https://sr.ht/~jpastuszek/multistream-batch/)
- [burstq](https://github.com/tedsta/burstq)
- [dataloader-rs (cksac)](https://github.com/cksac/dataloader-rs)
- [async-graphql dataloader](https://docs.rs/async-graphql/latest/async_graphql/dataloader/index.html)
- [Juniper DataLoader](https://graphql-rust.github.io/juniper/advanced/dataloader.html)
- [tower crate](https://docs.rs/tower)
- [tower Service trait](https://docs.rs/tower/latest/tower/trait.Service.html)
- [kameo](https://docs.rs/kameo/latest/kameo/)
- [Rust actor benchmarks and comparison](https://tqwewe.com/blog/comparing-rust-actor-libraries/)
- [salsa-rs/salsa](https://github.com/salsa-rs/salsa)
- [Bevy events cheatbook](https://bevy-cheatbook.github.io/programming/events.html)
- [effing-mad](https://github.com/rosefromthedead/effing-mad)
- [reffect](https://github.com/js2xxx/reffect)
- [eff](https://docs.rs/crate/eff/latest)
- [Haxl (facebook)](https://github.com/facebook/Haxl)
- [Fun With Haxl (Simon Marlow)](https://simonmar.github.io/posts/2015-10-20-Fun-With-Haxl-1.html)
- [ZIO executor locking](https://degoes.net/articles/zio-cats-effect)
- [Algebraic Handler Lookup in Koka, Eff, OCaml, Unison](https://interjectedfuture.com/algebraic-handler-lookup-in-koka-eff-ocaml-and-unison/)
- [OCaml 5 effect handlers manual](https://ocaml.org/manual/5.2/effects.html)
- [arc-swap](https://docs.rs/arc-swap)

Note: `haxl-rs` did not surface in any search; no active Rust port of
Haxl exists under that name on crates.io or GitHub as of this scan.
