# labkit — the golden harness (traitify every experiment)

"Can we traitify it so every experiment MUST report plan + space/time Big-O + all the
counters?" **Yes.** This crate is the answer, running.

```
cargo run --release --example grand_table          # the golden table, 5GB gun
cargo run --release --example grand_table --features with-dd   # + differential-dataflow (next slot)
```

## The trait

Every incremental engine implements one trait to enter the table:

```rust
pub trait Experiment {
    fn name(&self) -> &'static str;
    fn complexity(&self) -> Complexity;   // DECLARED time + space Big-O
    fn reset(&mut self);
    fn setup(&mut self, base_facts: usize);
    fn tick(&mut self, adds: &[i64], removes: &[i64]);
    fn digest(&self) -> i64;              // equivalence key
    fn live(&self) -> u64;
    fn total_rows(&self) -> u64 { self.live() }   // incl. history
    fn recompute_units(&self) -> u64 { 0 }        // full re-derivations
    fn writes(&self) -> u64 { 0 }                 // write ops (N+1 tripwire)
    fn plan_snapshot(&self) -> Option<String> { None }  // EXPLAIN QUERY PLAN
}
```

The `Harness` sweeps scales, runs every engine on the SAME deterministic edit stream,
counts everything, snapshots the plan, and — the key part — **fits the empirical Big-O**
(least-squares log-log slope of time and memory across scales) and prints it next to the
DECLARED complexity. A declared `O(1)` that measures slope ≈ 1.0 is visibly falsified.

**The one honest caveat:** Big-O is not measurable from a single run — it is inferred from
the sweep. So the trait cannot *derive* Big-O; it forces each experiment to *declare* it and
the harness *falsifies* the declaration against the measured slope. That is the store lab's
"measured, not asserted" discipline, made a trait obligation.

## The golden table (file-backed SQLite, 5 GB gun, 200 ticks × 100 edits)

| scale | engine | live | rows | recompute | writes | apply_ms | allocMB | equiv | meas t^p |
|---|---|---|---|---|---|---|---|---|---|
| 3M | ram-zset | 3.00M | 3.00M | 0 | 16585 | **1.0** | 102 | ✓ | ~0 (O(Δ)) |
| 3M | sqlite-temporal | 3.00M | 3.01M | 0 | **1300** | 437 | 70 | ✓ | 0.18 (O(Δ·log n)) |
| 3M | salsa-rows | 3.00M | 3.00M | **201** | 0 | **8951** | 182 | ✓ | **1.07** (O(facts)/tick) |

What the table proves in one place:

- **equivalence**: all three engines produce byte-identical digests at every scale (✓).
- **salsa in the fact role is quantifiably wrong**: apply time slope **1.07** (linear in facts)
  vs ram/sqlite ~flat. This is the earlier "salsa for control, not facts" finding, now a
  measured slope, not a claim.
- **N+1 tripwire**: sqlite issues ~5 statements/tick regardless of Δ or scale (writes flat
  ~1300), the set-based win; ram is O(Δ) writes; salsa does full recomputes (recompute 201).
- **the harness falsifies**: sqlite declared `O(working set)` space but the in-process
  `allocMB` slope is ~0.88 — flagged. The clean per-process number (temporal-lab: 31 MB @ 3M
  file-backed) shows RSS is actually bounded; a single shared process can't fully isolate
  memory, and the harness surfacing the gap is the mechanism working.

## The gun (5 GB, always to the head of alloc)

`gun::Gun` is a `#[global_allocator]` ported from `sprefa-store::memcap::CappedAlloc` (proven
with memcap_probe): counts live bytes, returns null past the cap → Rust `handle_alloc_error`
→ clean SIGABRT, on every platform. `gun::install(5120)` sets it plus a Linux `setrlimit` belt.
**Caveat (the store's RAM audit):** the gun sees only the RUST heap; SQLite's C allocator is
invisible, so the sqlite experiment also sets `PRAGMA soft_heap_limit`.

## Rational setup — every test states one

Each experiment implements `rationale()` (what real mechanism it models, so the setup is
arguable) and each `Workload` carries a `rationale`. The harness prints them before the table.
Two workloads:

- **live-set** — the fact layer under edit churn (uniform, 2x pool). Answers "what edges exist".
- **reachability (blast radius)** — the REAL product query: all-pairs transitive closure of a
  sparse call-graph DAG, with LOCALIZED edits (one function's out-edges rewritten per tick, the
  true change shape). Answers "what is reachable".

## The reachability table (the real query, `cargo run --release --example reach_table`)

Scales are node counts (a per-module call graph is hundreds of functions); TC is O(V·E).

| engine | declared | **measured t^p** | 100 nodes | 800 nodes | equiv |
|---|---|---|---|---|---|
| ram-reach (BFS recompute) | O(V·E)/tick | **1.81** | 6.8 ms | 285 ms | ✓ |
| sqlite-reach (recursive CTE) | O(reach)/tick | **2.10** | 85 ms | 6632 ms (~23×) | ✓ |

Recompute-from-scratch TC is super-linear per edit — the motivation for the incremental actor.
**The equivalence check earned its keep**: chasing three-way agreement (ram == sqlite == oracle)
found and fixed three real bugs — a u64→i64 `%` sign flip producing negative node ids, an oracle
that drifted from the emitted stream (fixed by defining it as a replay), and duplicate initial
edges (weight 2 vs 1 on removal). Equivalence is not decoration; it caught them.

## Slots ready, not yet filled

- **differential-dataflow / DBSP** (the incremental payoff): the actor that would flatten the
  reach slope from ~2.0 to ~0 (O(Δ) per edit). The trait accommodates it; port is the store's
  `dd_reach.rs` `iterate()` for TC, fed per-tick deltas. `with-dd` feature + versions wired.
- **the store's semi-naive DRed cascade** (frontier→hits→next, `cascade.rs`) as a third reach
  actor — the on-disk incremental TC with retraction, `recompute_units` = fixpoint rounds.
