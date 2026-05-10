---
name: sprf-dd-retractable-effects
description: [v4 planning] Three regimes for effects in DD-backed sprefa — idempotent terminal, debounced commit, paused yield. Effects-as-collections so retraction propagates the same way relational state does. Load when designing how an op's effect side fires and cancels, or when implementing yield/await/debounce.
---

# Retractable effects via collections

## The shift

```
today                          proposed
─────                          ────────
sink ──► effect_runtime         sink ──► effect_pending  (a Collection)
──────────► tokio               ──────────► debounce(K rounds)
──────────► I/O fires           ──────────► commit       (a Collection)
                                ──────────► tokio
                                ──────────► I/O fires
```

Effect descriptions become first-class collections. They live in the dataflow until they commit. Retraction at any stage cancels them.

## Three effect regimes

| Regime | Examples | Cancel mechanism |
|--------|----------|------------------|
| **1. Idempotent terminal** | publish_diag, set-semantic write | sink fires on +1 → publish; on -1 → clear (different effect kind, symmetric) |
| **2. Debounced commit** | write file, send LSP edit | effect sits with +1 for K rounds; if retracted before T+K, never fires |
| **3. Paused yield** | yield/await/sh-approve | pending_yield bag; retracting = automatic unsubscribe |

## Regime 2: Debounced commit wiring

```rust
effect_pending: Collection<G, (EffectDesc, T_emit)>

effect_committed = effect_pending
    .filter(|(_e, t_emit)| t_emit + K <= current_frontier())

effect_committed.inspect_batch(|t, batch| {
    for (eff, _t_emit), _t, diff in batch:
        if diff > 0: tokio.dispatch(eff)
        else:        tokio.compensate(eff)
});

effect_pending.consolidate()  // drops cancelled +1/-1 pairs
```

Timeline (K=2):

```
T=0:   user types.  effect_pending +1  for "write hello.txt"
T=1:   user types more. effect_pending -1, +1 (replaced)
T=2:   user types more. effect_pending -1, +1
T=3:   user idles.   t_emit=2, frontier=3, 2+K=4. NOT YET.
T=4:   still idle.   2+2 ≤ 4. effect_committed +1. tokio fires.

contrast: at T=3 user reverts → effect_pending -1
          consolidate drops (T=2,+1) + (T=3,-1) → net 0
          effect_committed never sees the row. zero I/O.
```

## Regime 3: Paused yield wiring

Mirror SubjectRegistry as DD collections so retraction drives lifecycle.

```rust
pending_yield: Collection<G, (yield_key, upstream_row_key, payload_tag, lineage)>
resolved:      Collection<G, (yield_key, payload)>
timed_out:     Collection<G, yield_key>      // injected by runner at deadline

active = pending_yield
       .antijoin(resolved.map(|(k,_)| k))
       .antijoin(upstream_bag_for_row_key)   // upstream gone → yield gone
       .antijoin(timed_out)                   // deadline passed → yield gone

active.inspect_batch(|t, batch| {
    for (key, _t, diff) in batch:
        if diff > 0: registry.subscribe(key);
        else:        registry.unsubscribe(key);
});
```

Retraction propagation handles unsubscribe automatically:

```
upstream_row -1   →  pending_yield -1
                       → registry.unsubscribe(yield_key)
                       → tokio future resolves Err(Unsubscribed)
```

## Properties preserved

- ✓ purity                ops emit effect descriptions, never run I/O
- ✓ retraction-correct    -1 at any stage cancels downstream
- ✓ debounce by frontier  no per-effect timer, the dataflow IS the timer
- ✓ replay determinism    input log + K + scope = same effect log
- ✓ cancel by lineage     antijoin against newer-version Collection
- ✓ idempotence           consolidate collapses (+1, -1) automatically
- ✓ structured logging    pending and committed both queryable bags

## What does NOT compress

| Case | Why |
|------|-----|
| Network sends already fired | Once a row crosses inspect_batch into tokio, it has left the dataflow. Cancel needs a compensating effect kind (DELETE for POST), not a -1. |
| Long-running external work  | Use the Yield primitive. External code calls `next` when ready. |
| Non-deterministic effect content | Pin the non-determinism at the input seam, not in the op. |

## The boundary rule

> Once a row crosses inspect_batch into tokio, it has left the dataflow. Cancellations after that need a compensating effect kind, not a -1.

In sprefa vocabulary:

| New verb     | Old verb                    |
|--------------|-----------------------------|
| pending      | yield / write_pending / sh_approve |
| commit       | put / next / dispatch       |
| cancel       | unsubscribe / retract / takeLatest |
| compensate   | post-commit reverse-effect rows |

## Sources

- chat_log/20260501.0.dd-mental-model-walkthrough.md (loop avoidance + effect graph sections)
- chat_log/20260501.1.dd-effects-control-flow-types.md (three regimes)
- v3/crates/effect_runtime/src/subjects.rs (existing SubjectRegistry / Yield primitive)
