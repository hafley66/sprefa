---
name: sprf-dd-purity-and-durability
description: [v4 planning] Operator purity contract for sprefa over DD, three impurity seams, five durability properties that fall out of the contract. Load when designing op closures, dynamic lowering from IR to dataflow, or when reasoning about replay/checkpoint guarantees.
---

# DD purity contract for sprefa

## The contract

Every operator closure must satisfy:

1. **Deterministic** — same row in, same value out, every time
2. **Total** — never panics on any well-typed row
3. **Side-effect free** — no I/O, no mutation visible outside the closure
4. **Pure of time** — no `SystemTime::now`, no `Instant`, no env reads
5. **Captures only what is in the closure's environment at build time**

If every operator closure satisfies these, then:

- Trace at any time T is fully reconstructible from `(input log up to T) + (operator graph spec)`.
- Operator state is a *cache*, not source of truth. You can throw it away and rebuild.
- The output log at T is determined by the input log up to T.

## Three seams of impurity

```
INPUT SEAM
    rayon parses, diffs, calls input.update_at(row, T, ±1).
    this mutates the InputSession's pending queue.

PURE CORE
    filter, map, join, antijoin, reduce, iterate, distinct.
    every operator a closure, every state an arrangement.
    determinism: replay input log → same output log.

OUTPUT SEAM
    inspect_batch(|t, batch| sender.send(batch.clone()))
    only callback in the graph that does I/O.
    pushes effect descriptions to a queue.

EFFECT SEAM
    tokio receives, dispatches PublishDiag/Write/...
    effects are descriptions first, executed second.
    can be retried, batched, deduplicated.
```

Each seam is a place to checkpoint, log, or replay independently.

## Dynamic lowering shape

Script is data, not Rust source. `lower(ir)` walks the IR and calls timely operator methods with closures that embed the IR's compiled bits.

```rust
fn lower_op(ir: &OpIR, c: Collection<G, Row>) -> Collection<G, Row> {
    match ir {
        OpIR::Filter { capture, regex } => {
            let rx = Regex::new(regex).expect("validated at parse time");
            let cap: Arc<str> = capture.clone();
            c.filter(move |row| {
                row_get(row, &cap).map_or(false, |v| rx.is_match(v))
            })
        }
        // ...
    }
}
```

After `lower_op` returns, the IR can be dropped; the closure owns its inputs. **The graph is a pile of closures connected by streams.** Lowering produces closures, closures do not produce lowering. No closure inspects IR at runtime.

## Five durability properties that fall out

1. **Input log = source of truth.** Persist only `Vec<(FactName, Row, T, ±1)>`. Throw away arrangements, restart, replay log into the freshly-built dataflow. End up at the exact same state.

2. **Operator graph swap.** Edit the script. Rebuild the dataflow with new closures. Replay the same input log into the new graph. New rules now compute their state from the same source data. No data migration.

3. **Time-travel queries.** `trace.cursor_through(T)` walks the trace at any historical time. Operators are pure → trace's view at T is what was true at T. Bounded by compaction frontier.

4. **Splittable replay.** Replay a prefix of the input log to catch up to a checkpoint, then attach the live event stream. Useful for cold-start, lagged replicas, forensic re-run.

5. **Effect idempotence by construction.** Each effect description carries `(row, T, diff)`. Effect runtime can dedupe by `(row, T)`. Replay → same effect descriptions → either coalesced or run-once.

## What you lose if you break the contract

| Violation                        | Loses                                        |
|----------------------------------|----------------------------------------------|
| `SystemTime::now()` in closure   | Replay determinism. Trace at T depends on wall-clock at original run. |
| Mutation of external state       | Input log alone cannot reconstruct. State diverges. |
| Panic on some rows               | Totality. Replay either dies or skips, both wrong. |

Operator-author rule, one line: **a closure may read its captured constants and the row, do CPU work, and return a value.** Anything else is an effect; it lives at the output seam, not in the closure.

## Where impurity legitimately lives

| Seam                  | What lives there                                 |
|-----------------------|--------------------------------------------------|
| Input (rayon side)    | parse, ast-grep extract, diff vs last_known, push |
| Output (sink side)    | inspect_batch sends to a channel                 |
| Effect (tokio side)   | LSP push, SQLite write, fs write, sh approval    |
| Pure cached effects   | `PureEffect` impl with cache_key + DOMAIN; result memoized in effect_runtime |

## Sources

- chat_log/20260501.0.dd-mental-model-walkthrough.md (Purity Contract section)
- chat_log/20260501.1.dd-effects-control-flow-types.md
- v3/crates/effect_runtime/src/lib.rs (RtCtx, PureEffect, Batcher)
