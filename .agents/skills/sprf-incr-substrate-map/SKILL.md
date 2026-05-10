---
name: sprf-incr-substrate-map
description: [v4 planning] Workload → substrate map for sprefa — Salsa vs effect_runtime vs timely/DD. Which incremental tech owns which job. Load when adding a feature and unsure which substrate to put it in.
---

# Three substrates, three jobs

## Comparison

```
                  effect_runtime          Salsa                 timely / DD
                  (yours, today)          (ra, kanata, ...)     (frank/timely)
   ─────────────  ─────────────────────  ──────────────────    ───────────────
   unit           Effect (data)           tracked fn            Differential collection
   interpreter    Batcher<E>              query fn body         operator graph
   purity         PureEffect marker       pure assumption       pure operators
   memoization    CacheLayer (per-E)      per-call memo         none (always recompute deltas)
   invalidation   DomainInvalidate        rev bump + early-cut  timestamp advance
   cancellation   CancellationToken ✓     Cancelled panic ✓     n/a (data-driven)
   dep graph      ✗                       implicit, automatic   explicit, dataflow shape
   replay log     ✗                       ✗                     ✓ (changelog topic)
   batching       Batcher trait ✓         no                    yes (operators)
```

Three regimes, three substrates. Most projects pick one; sprefa wants all three because the language has all three flavors of computation in it.

## Workload → substrate

```
   workload                            best fit                    salsa role
   ───────────────────────────────────────────────────────────────────────────
   many precise tiny rules             AC prefilter + bitset       none
   (ast-grep regime)                   rayon over files            (overhead/call too high)

   full-repo language check            rayon + arena, one-shot     none
   (biome/oxc on 1 repo)               drop tree at end            (memo never reused)

   cross-repo rules                    timely / DD                 partial overlap;
                                       deltas of facts             timely wins on deltas

   full-repo string indexing           one-shot scan → disk         none
   (SCIP-shape)                        load as cold inputs

   auto-refactoring writes             effect_runtime              none
                                       JournalLayer + Staging

   live-edit lower(file) → IR          OnceCell hand-roll, OR      single tracked query
   (LSP hot path)                      one tracked Salsa query     if ≥3 consumers
   ───────────────────────────────────────────────────────────────────────────
```

sprefa is **batch-shaped**, not interactive-shaped. Salsa wins when one source change invalidates a small slice of a giant memoized graph that many UI requests are waiting on. sprefa's tiny rules → bitset dispatch beats memo lookup; whole-repo passes → results consumed once and dropped; cross-repo → timely already does it better; indexing → write-once-read-many on-disk; writes → not a query layer.

## Hybrid architecture

```
                   ┌─────────────────────────────────────────────────────────────────┐
                   │  Salsa region   (per-file scope, HIGH/MED/LOW durability)        │
                   │     parse(f)  ──►  lower(f)  ──►  facts_emitted(f) : Vec<Fact>   │
                   └────────────────────────────┬─────────────────────────────────────┘
                                                │  (delta over prior)
                                                ▼
                   ┌─────────────────────────────────────────────────────────────────┐
                   │  Timely / DD region   (cross-file, cross-repo)                  │
                   │     Collection<Fact>  ──►  joined queries  ──►  rule outputs    │
                   └────────────────────────────┬─────────────────────────────────────┘
                                                │
                                                ▼
                                          tag relations,
                                          materialized rule results
                                                │
                                                ▼
                                       LSP reads, renders
```

Salsa owns "what does this file mean as code." Timely owns "what relations does this code participate in across the workspace." Edit one file → Salsa re-lowers it → lowered facts diffed against prior facts → diff into timely as a delta → timely propagates to dependent cross-repo queries. Salsa never sees the cross-repo dep.

## Where effect_runtime sits

```
                    one substrate, two roles:

   effect_runtime ┌──────────────────────────────────────────────┐
                  │  ctx.put(LowerEffect(file)).await             │  ← "Salsa-shaped"
                  │     ▼                                          │     (memo + dep edges
                  │  CacheLayer<LowerEffect>                       │     + cancellation)
                  │     ▼                                          │
                  │  inner: pure compute                           │
                  ├──────────────────────────────────────────────┤
                  │  ctx.put(WriteEffect(...)).await               │  ← "saga-shaped"
                  │     ▼                                          │     (journal + staging)
                  │  StagingBatcher                                │
                  │     ▼                                          │
                  │  buffered, surfaced to LSP                     │
                  └──────────────────────────────────────────────┘
```

Same `RtCtx`, same trait surface, two policies. Pure-compute effect (parse, lower, resolve) is what Salsa does; just expressed as an effect. The dep graph could be the call graph of the interpreter; the memo table could be a layer in the request pipeline. Salsa is most powerful when you don't already have a runtime; sprefa already has one.

## Substrate decision shape

```
   per-file IR cache for LSP hot path?           OnceCell, OR one Salsa query
                                                  (only if ≥3 consumers)

   pure effect with reusable result?             effect_runtime CacheLayer

   cross-file/cross-repo relational query?       timely / DD

   batch one-shot whole-repo pass?               rayon + arena, no memo

   stage/preview/journal effectful writes?       effect_runtime + StagingBatcher
                                                  + JournalLayer

   pre-index 500 read-only repos?                one-shot scan → disk → load as
                                                  HIGH durability inputs (SCIP-shape)
```

## Concrete next moves

1. `JournalLayer<E>` wraps any Batcher; appends `(EffectId, E, Response)` per call. ~80 LoC.
2. `StagingBatcher` for `WriteEffect` buffers into a `Store`. LSP reads the store. ~100 LoC.
3. Scope-tracked dep edges in `RtCtx::put` so cache invalidation gets early cutoff. ~150 LoC.
4. Pick *one* Salsa boundary if/when ≥3 consumers demand lowered IR for the same file: `fn lower(db, file) -> Arc<Pipeline>`. Don't Salsa-ify the whole IR at once.
5. Defer timely until tag relations actually feel slow; the Datalog layer can be a synchronous fold over Salsa-cached lowered facts for now.
