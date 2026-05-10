---
name: sprf-dd-substrate-shape
description: [v4 planning] Differential Dataflow as state backbone for sprefa — three thread pools, daemon loop shape, scope/sinks/probe/frontier mental model, what DD owns vs what it doesn't. Load when designing the runtime, deciding what goes in DD vs effect_runtime vs rayon, or before integrating DD into pipeline crate.
---

# DD as state backbone for sprefa

## What DD owns

| Capability                  | DD gives | You write |
|-----------------------------|----------|-----------|
| Row storage (bags)          | Yes (Trace + arrange) | — |
| Set semantics, dedup        | Yes (`distinct`, consolidation) | — |
| Append + retract            | Yes (`InputSession`)  | — |
| Join (semi/anti/equi)       | Yes | — |
| Recursion / fixpoint        | Yes (`iterate`) | — |
| Aggregation                 | Yes (`reduce`, `count`, `threshold`) | — |
| Diff propagation            | Yes (the whole point) | — |
| Generation/round seal       | Yes (frontier + step_while) | — |
| Multi-worker sharding       | Yes (key hash exchange) | — |

## What DD does NOT own

| Capability                  | You write |
|-----------------------------|-----------|
| Persistence                 | SQLite sink per fact, ~250 LoC |
| Cold-start replay           | Loader reading SQLite at gen 0, ~80 LoC |
| Schema / dynamic widening   | Either codegen per script, or `Vec<Arc<str>>` rows + side schema map |
| Async I/O                   | Stage in tokio first, then push to InputSession |
| Cross-process distribution  | Out of scope, or run multiple sprefa processes |
| Cursor flow / pattern match | Existing ast-grep+rayon stage |
| Effect dispatch (LSP, write)| Existing effect_runtime |
| Trace compaction policy     | DD has the mechanism; you set the cadence |

## Three thread pools, three jobs

```
RAYON   parse + diff,         N threads, hot during file events
TIMELY  the wired graph,      W threads (often 1), single-tick sync
TOKIO   effect dispatch,      N/2 threads, mostly idle
```

Rule: **never `.await` while holding a CPU-pool thread.** Dispatch via `pool.spawn + oneshot::channel`. Three pools competing for the same N cores oversubscribe; budget tokio = N/4, rayon = N/2, timely = N/2 as a starting split.

## Scope / Sinks / Probe

| Term  | Built when           | Role |
|-------|----------------------|------|
| Scope | Process start, once  | The dataflow graph. Contains all operators. |
| Sinks | Inside the scope     | Where DD records exit into your runtime. |
| Probe | Inside the scope     | Frontier observer. Tells outer loop when round T is done. |

```rust
let (mut input, probe) = worker.dataflow::<usize, _, _>(|scope| {
    let mut input = InputSession::new();
    let edit = input.to_collection(scope);
    let long = edit.filter(|(_p,_l,t)| t.len() >= 80).map(|(p,l,_)| (p,l));
    long.inspect_batch(|t, batch| { /* sink: emit effect descriptions */ });
    let probe = long.probe();
    (input, probe)
});
```

## Daemon loop shape

```
boot:
    parse script → IR
    build_scope(ir) → (input_sessions, probe, sinks)
    replay SQLite into InputSessions at T=0
    worker.advance_to(1); step_while(probe < 1)
    READY

per file-change event:
    1. rayon: re-parse changed file, diff vs last_known set → (adds, retracts)
    2. for each: input_session.update_at(row, T, ±1)
    3. T += 1; advance_to(T); flush
    4. step_while(probe.less_than(T))
    5. sinks have already fired during step_while; runner drains effect queue
    6. tokio dispatches batched effects (LSP, SQLite, fs writes)
```

The dataflow graph is built ONCE. Rounds are `advance_to + flush + step_while`.

## When to use DD vs effect_runtime vs rayon

| Workload                                | Goes in       |
|-----------------------------------------|---------------|
| Pure CPU work (parse, regex, hash)      | rayon         |
| Pure cached effect (read bytes, ls dir) | effect_runtime PureEffect |
| Relational state (facts, joins, antijoins, retraction-aware) | DD |
| Async I/O (LSP socket, fs write, sh)    | effect_runtime + tokio |
| Stateful pause (yield/await user input) | DD pending_yield + SubjectRegistry sink |
| Debounced commit (write file after K rounds idle) | DD effect_pending Collection |

Rule: **the InputSession is the dam.** Pure stuff fills the reservoir on rayon. DD pipes it out with retraction-correct flow. tokio drinks downstream.

## Honest cost estimate

Adopting DD as state backbone:
- Delete: ~1850 LoC (`RelationStore`, custom join/antijoin, RAII writer-share, seal_waiters, future stratification analyzer).
- Add: ~780 LoC bridge (`dd_scope`, `dd_row`, `dd_runner`, `dd_persist`).
- Net: ~−1000 LoC plus correctness wins on retraction.

Real numbers come from integration + benchmark. Don't claim them before measuring.

## Sources

- DD/timely 0.12 + ~/projects/ext/sprefa-dd-poc/src/{main.rs, bin/larger.rs} 3-gen retraction POC
- chat_log/20260430.0.dd-poc-incremental-scanner-design-vector.md
- chat_log/20260501.0.dd-mental-model-walkthrough.md
- v3/crates/effect_runtime/src/{lib.rs, subjects.rs}
- v3/crates/pipeline/src/relation_store.rs (the surface to replace)
