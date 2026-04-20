# Appendix: Effect-dispatcher convergent evolution

Four independent lineages, across four ecosystems, over a decade,
arrived at the same surface.

| year | ecosystem | who | surface |
|---|---|---|---|
| 2014 | Haskell | Haxl (Marlow et al.) | `DataSource` class: request type + response type + `fetchBatch` |
| 2016 | JS / web | redux-saga (Yelouafi) | `yield put(action)` returns typed response; middleware dispatches |
| 2019 | Rust | tower (Tokio team) | `Service<Req>`: request type + associated `Response` type + `call` |
| 2024 | sprefa v3 | (this design) | `EffectKind`: request struct + `type Response` + `Batcher<E>` registry |

## The unifying observation

> **Effect = request type + response type + dispatcher.**

Once that shape is committed to, five cross-cutting problems
dissolve into the one framework:

- **async-trait problem** — one `dyn` cell in the registry, not
  many at every service boundary
- **batching problem** — dispatcher decides policy
  (passthrough / opportunistic / windowed / adaptive) per kind
- **N+1 problem** — no singular API to accidentally loop over,
  because every call goes through `put` / `yield put` /
  `service.call` / `fetchBatch`
- **cancellation problem** — stays orthogonal, as it should
  (CancellationToken / AbortController / `context.Context` / STM)
- **push-pull problem** — dispatcher emits a pull-native stream;
  push only at edges

## Why this matters

The design is not novel. It is the **convergent shape four
ecosystems independently found** when forced to solve
(effect reification × async × batching × cancellation × N+1)
in one coherent story.

Rust took until ~2023 (native `async fn` in trait, Dec 2023) to
have the type-system ergonomics to express this without fighting
the language. Haskell had it in 2014. JS had it in 2016 via
generators. Tower had it in 2019 by refusing to use
`async fn in trait` and going straight to associated-type
futures.

The sprefa v3 shape inherits the tower ergonomics and the Haxl
semantics. Four lineages, one surface — that is the unification.

## The four surfaces side by side

```haskell
-- Haxl (Haskell, 2014)
class DataSource req where
  fetch :: State req -> [BlockedFetch req] -> IO ()
dataFetch :: DataSource req => req a -> GenHaxl u a
```

```javascript
// redux-saga (JS, 2016)
function* saga() {
  const result = yield put({ type: 'FETCH_USER', id })
  // middleware sees the action, dispatches it, returns result
}
```

```rust
// tower (Rust, 2019)
trait Service<Request> {
    type Response;
    type Future: Future<Output = Result<Self::Response, _>>;
    fn call(&mut self, req: Request) -> Self::Future;
}
```

```rust
// sprefa v3 (2024)
trait EffectKind: Send + 'static {
    type Response: Send + 'static;
}
impl RtCtx {
    pub async fn put<E: EffectKind>(&self, e: E) -> E::Response;
}
```

## The three dyn boundaries in sprefa v3

Only three places where type erasure lives. Everything else is
monomorphized per op.

1. `Captures :: Vec<Box<dyn CaptureKind>>` — per cursor, for
   heterogeneous capture payloads
2. `Slots    :: HashMap<TypeId, Box<dyn Any>>` — per cursor, for
   op-owned typed scratch
3. `Registry :: HashMap<TypeId, Arc<dyn BatcherEntry>>` — one
   global, the one unavoidable dyn for effect dispatch

That is the entire type-erasure budget. Associated types handle
the rest.

## Reading order

1. Haxl paper (Marlow 2014) — the machine
2. Tower "Inventing the Service trait" blog post — the Rust idiom
3. Build Systems à la Carte (Mokhov 2018) — the applicative vs
   monadic formalism for when batching works
4. Swierstra, *Data types à la carte* (2008) — effects as ADTs
5. `chat_log/20260418.1.effect-batching-prior-art.md` — the full
   lineage and citations
6. `chat_log/20260418.2.v3-design-and-numbers.md` — the concrete
   LoC delta, per-effect policy, migration path
