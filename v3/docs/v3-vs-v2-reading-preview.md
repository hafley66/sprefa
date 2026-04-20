# Appendix: v3 vs v2 reading preview

A preview of what code looks like in each layer, v2 today vs v3
proposed. The metric that matters: **what does the op author
read 95% of the time?**

---

## Reading an op in v3 (the 95% case)

```rust
// ops/ast_grep.rs — one file owns everything for this op
pub struct AgMatch { pub pat: Arc<Pattern>, pub at: ByteRangeRef }
impl EffectKind for AgMatch { type Response = Vec<NodeHit>; }

pub struct AstGrepOp;
impl Op for AstGrepOp {
    type Cap    = AgCap;
    type Effect = AgMatch;
    type Slots  = (AgTreeSlot,);

    fn pipe(&self, ctx: &RtCtx, s: CursorStream) -> CursorStream {
        s.then(|batch| async move {
            let hits = ctx.put(AgMatch {
                pat: self.pat.clone(),
                at:  batch.at(),
            }).await;
            emit_cursors(hits, batch)
        })
    }
}
```

One file. Four types declared together. One `pipe` body reads as
a straight pull pipeline with effects yielded at `put`. The op
author never writes `join_all`, `block_on`, `Arc<Mutex<_>>`,
`async_trait`, or a custom error enum. The effects their op uses
show up in the type signature (`type Effect`) so a reader knows
the blast radius without hunting.

Verdict: **delightful**.

---

## Reading the framework (the 5% case)

```rust
trait EffectKind: Send + 'static {
    type Response: Send + 'static;
}

trait BatcherEntry: Send + Sync {
    fn submit(&self, req: Box<dyn Any + Send>)
        -> BoxFuture<Box<dyn Any + Send>>;
}

impl RtCtx {
    pub async fn put<E: EffectKind>(&self, e: E) -> E::Response {
        let entry = self.registry
            .get(&TypeId::of::<E>())
            .expect("batcher registered");
        let any_in:  Box<dyn Any + Send> = Box::new(e);
        let any_out = entry.submit(any_in).await;
        *any_out.downcast::<E::Response>()
            .expect("type invariant")
    }
}
```

The `Any` + `downcast` dance is the cost of "one registry, N
effect kinds, no central enum." ~40 lines total, lives in one
file, nobody reads it after writing it once.

Verdict: **tolerable**.

---

## The one genuine papercut

`async fn` inside a trait with generics, plus associated types,
plus `Send + 'static` bounds, plus `?Sized` for dyn compat —
stacks up:

```rust
pub async fn put<E: EffectKind>(&self, e: E) -> E::Response
where
    E: Send + 'static,
    E::Response: Send + 'static,
{ ... }
```

Every generic signature has a where-clause ladder. Rust 1.75
`async fn` in trait helps but doesn't remove the `Send` bounds.
This is the tax for "monomorphized, no boxing, no async_trait
macro." Framework code wears it; ops don't.

---

## Side by side: hover feature

### v2 today (condensed, real code shape)

```rust
async fn hover_at(&self, uri: &Url, pos: Position) -> Option<Hover> {
    let bytes = self.reader
        .bytes(&repo, &rev, &fp)
        .next()
        .await?;
    let tree = self.reader
        .parsed(&repo, &rev, &fp, kind)
        .next()
        .await?;
    let rows = self.store
        .query_expr(name, where_)
        .await
        .ok()?;
    // ... Arc::clone dance, block_on in test, Vec::new fallback everywhere
}
```

Three vocabularies (`reader.X().next().await`,
`store.X().await.ok()?`, plus error flattening). Each surface
exposes its own error type + its own return-shape quirks
(`BoxStream<Bytes>` vs `Result<Vec<Row>, StoreErr>`).

### v3 proposed

```rust
async fn hover_at(&self, ctx: &RtCtx, at: ByteRangeRef) -> Option<Hover> {
    let bytes = ctx.put(ReadBytes  { at }).await;
    let tree  = ctx.put(ReadParsed { at, kind }).await;
    let rows  = ctx.put(QueryStore { name, where_ }).await;
    // that's the function
}
```

One vocabulary (`put`). One await shape. No `.next()`, no
`Arc::clone`, no `.ok()?` chains hiding error downcasts. The
effect kinds tell the story — a reader traces `ReadBytes` back
to `effects/read_bytes.rs` to see its batcher policy, and that
is a separate concern from hover logic.

---

## What gets easier

- **op code**: one vocabulary, effects visible in types, no
  `join_all` / `block_on` / `Arc<Mutex>` in pipe bodies.
- **tests**: `RtCtx::for_test` registers mock batchers; no
  `MemReader` + `MockStore` + `NoopWriter` + `AutoApprove`
  plumbing per test.
- **adding a new op**: one folder. Own effect kinds, own
  batchers, own capture kind, own diagnostic — all colocated.
- **adding a new effect**: one file. No `enum` to grow. No
  trait method to add across four impls.

## What gets harder

- **framework code**: ~500 lines of type gymnastics (TypeId +
  Any + downcast). Touched rarely but dense when touched.
- **generic signatures**: `Send + 'static` bound ladder
  uniformly present. Uniform, but noisy.
- **debugging**: effect goes through batcher, stack trace has
  an extra frame. Mitigate with `tracing::instrument` on every
  `put`.
- **intra-op dependencies**: sequential `.await` on two `put`
  calls works but loses applicative collapse across ops. Fix
  with `put_many(&[e1, e2, e3])` for independent fetches.

---

## Verdict

The **op layer reads like pseudocode with types.** That is the
metric that matters, because that is what contributors spend 95%
of their time in.

The framework tax is real, paid once, and lives in one file.

See also:
- `appendix/convergent-evolution-effect-dispatcher.md` — why four
  ecosystems converged on this shape
- `chat_log/20260418.2.v3-design-and-numbers.md` — LoC delta and
  migration path
