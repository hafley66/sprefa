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

---

## Evolution notes 2026-04-20 — language-surface read

Cross-reference: `v3-unified-language-locks.md`. The Rust op-reading
example above is unchanged; this section adds the sprf-author reading.

### Reading a rule in v3

```sprf
rule(classes) > ast[rust] { class $NAME }

rule(used_by, $CLASS) > ast[rust] { new $CLASS() }

rule(audit) {
  > classes
  > used_by($NAME)
  > is_repo_norm($NAME)
  > link(:class_used_in, $NAME, :at_repo, $REPO)
}
```

One file. Three rules. Parametric `used_by` lazy until called. Tag-op
`is_repo_norm` annotates `$NAME` as a normalized repo-pointer. Link-op
emits a relation row. No sigils beyond `$`. No control-flow keywords.

### What a reader sees at each line

| line | what's happening |
|---|---|
| `rule(classes)` | zero-param rule, auto-subscribed by runner, produces `classes` sqlite table |
| `rule(used_by, $CLASS)` | one-param rule, lazy; `$CLASS` is an input term the body pattern-holes |
| `rule(audit) { ... }` | zero-param rule driving the composition |
| `> classes` | subscribe the classes stream; emits cursors with `$NAME` bound |
| `> used_by($NAME)` | call with bound `$NAME` → filter mode; emits cursors where a `new $NAME()` call exists |
| `> is_repo_norm($NAME)` | tag-op; writes relation row; cursor passes through |
| `> link(:class_used_in, $NAME, :at_repo, $REPO)` | link-op; writes relation row with two terms |

### Writing a rule in v3

The rule author fills zero trait slots. They write composition.
Framework handles everything else:

- arg-mode dispatch derived from op ArgSpecs
- binding-source DAG checked at lower time
- parametric-rule invocation wiring
- relation-row persistence
- evidence tables per stage
- subscribe policy (defaults to Shared for named rules)

### Framework reading (the language-lockfile 5% case)

Framework author reads `v3-unified-language-locks.md` once for the six
concepts, three sigils, six Cursor fields, four Pipeline cases, five
EntityRef cases. Op author never opens that file.

### What changed from the teaching doc

`v2/docs/_b_v3-unified-language.md` Chapter 7 treats `${...}` as the
cross-rule-ref form with `>` rename. Session re-locked: `${...}` is the
host-expr carveout (general), rename is `> $NAME` capture-write station
(a plain chain op). Carveout no longer special-cases rename; capture-write
is the mechanism.

`v2/docs/_b_v3-unified-language.md` Appendix A still shows the earlier
grammar. Grammar.js work blocked on:

1. Term-annotation shape (lockfile Section 14)
2. Anonymous pipeline AST normalization
3. Rule recursion attribute spelling

Once those pick, Appendix A gets its rewrite and grammar.js follows.

### Side by side: same audit, v2 today vs v3

**v2 today:**

```
rule(audit) {
  > ast[rust] {
      class $NAME
    }
  > $$repo_norm($NAME)
  > $$link(:class_used_in, $NAME, $$repo($REPO))
}
```

Three sigil classes (`$`, `$$`, `${...}`). Scan-pointer and link as
sigil operators. Mode of arg not surfaced.

**v3 proposed:**

```
rule(audit) {
  > ast[rust] { class $NAME }
  > is_repo_norm($NAME)
  > link(:class_used_in, $NAME, :at_repo, $REPO)
}
```

One sigil class (`$`). Scan-pointer and link as ordinary ops.
Arg-mode dispatch inferred.

### Verdict

The **sprf layer reads like a datalog dialect with stream composition.**
That is the metric that matters for author-facing docs. The Rust
op-reading layer is separately optimized, unchanged.

The prolog tax is real (arg-mode dispatch per op) and lives in the op's
`dispatch` function. Op authors wear it only when their op cares about
groundness; most ops use `Either` default and don't see it.
