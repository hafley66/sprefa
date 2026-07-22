# frp-lab — is "FRP the edge, batch the core" real, or just an assertion?

The v6 fork: write the datalog core in streams/FRP, or keep it batch-relational and
put FRP only at the event edge? The claim was that the cost is **lopsided** — max
Rust pain / low payoff in the core, bounded pain / max payoff at the edge. This lab
turns that claim into running code so it can be felt (or broken).

Three pieces, one screen of domain each:

| piece | what it is | result |
|---|---|---|
| `examples/core_batch.rs`      | CORE as rayon batch over borrowed rows | **compiles, runs, fast** |
| `examples/edge.rs`            | EDGE as a `futures::Stream` trigger    | **compiles, runs, clean** |
| `examples/core_incremental.rs`| the batch family's BREAK POINT, measured | full recompute O(corpus)/edit; Z-set delta ~17,000× faster |
| `examples/core_scale.rs` + `scale.sh` | 10M edges, skewed families, per-process RSS | break is RAM: string 2.7GB, dense 1.6GB @ 10M |
| `tests/oracle.rs`             | differential spec: incremental == batch recompute | 20k states pass; catches naive delta |
| `examples/core_frp_attempt.rs`| CORE as a stream graph on a scheduler  | **does not compile** (gated) |

## Reproduce

```
cargo run  --example core_batch                  # 7 edges, rayon, owned result, no lifetime
cargo run  --example edge                         # 3 coalesced jobs from a noisy event log
cargo run  --release --example core_incremental   # the break point, with timings
./examples/scale.sh                               # 10M edges, skewed families, clean RSS
cargo test --release --test oracle                # differential spec, 20k states
cargo build --example core_frp_attempt --features frp-core-attempt   # the wall
```

## The break point — the first lab strawmanned "batch" as full recompute

`derive_family_batch` re-reads EVERY file to answer ANY change. Measured
(`--release`, this machine):

| corpus | full build | one-file edit, full recompute | one-file edit, Z-set delta |
|---|---|---|---|
| 1k files  | 2.32 ms | 2.47 ms | — |
| 10k       | 23.8 ms | 19.2 ms | — |
| 100k      | 194 ms  | **213 ms** | **12.2 µs** |

A one-file edit costs the same as building the world — O(corpus) per edit. The
"family is terrible" claim is **correct**, and this is why: full recompute has no
delta.

The fix, and its honest shape:

- **naive-set delta is a correctness bug.** Keep a plain `BTreeSet<Edge>`, remove the
  changed file's old edges, add its new ones — and any edge two files both produce is
  dropped when either file changes (Finding 2 asserts this failing). A fact must
  survive until its LAST source retracts it.
- **Z-set (weighted) delta is correct and cheap.** `edge -> multiplicity`; a shared
  edge lives until its count hits zero. One-file delta over a 100k-file corpus:
  **12.2 µs vs 213 ms full recompute — ~17,000×.** This is the store lab's weight
  cascade (E0-E7) in miniature. It is **owned, batch-per-tick, no rxRust** — streaming
  the rows never enters either fix.
- **recursion is the remaining cost, and it is still not streams.** The Z-set above
  handles a non-recursive rule. A delta to a base edge in a recursive rule
  (transitive closure) cascades — O(delta × reachable) — which is semi-naive fixpoint
  over the Z-set, i.e. the on-disk weight cascade the store lab already measured
  (retract = O(delta·log n)) and already found beats dd/dbsp past the memory wall.

So the break point does not rescue FRP-core. It moves the core from "rayon
full-recompute family" (terrible) to "Z-set/semi-naive IVM" (the store cascade) —
which is a MORE batch-relational answer, not a more streamed one.

## The oracle (because hand-picked asserts are not a spec)

The Finding-2/3 asserts above are cases I happened to think of — not a spec. The real
spec for an incremental engine is **differential**: for every reachable corpus state,
`incremental.edges()` must equal a from-scratch recompute of the same files. That is
the store lab's "4-engine byte-identical" check. `tests/oracle.rs` drives a random
edit stream (add / modify / delete over a small path+symbol pool so shared edges and
multiplicities occur) and cross-checks after EVERY step.

```
cargo test --release --test oracle
```

- `zset_equals_batch_oracle_over_random_edits`: 50 seeds × 400 steps = **20,000
  cross-checked corpus states**, Z-set == batch recompute at every one.
- `naive_set_is_caught_by_the_oracle`: proves the oracle has TEETH — it flags the
  naive-set delta with no case hand-crafted by me, first at **seed 0, step 11**.

The oracle discriminates a known-wrong engine from the right one, so passing it means
something. It is also the correctness gate any real IVM core must keep.

## Scale — the break is memory, not correctness or time

`examples/scale.sh` runs each scenario in its OWN process (a first cut measured peak
RSS across scenarios in one process and mis-attributed a full-recompute's 1.7GB to the
Z-set — fixed). ~10M edges, three skewed families (Rust giant / Ts medium / Other tiny
scatter), `--release`, this machine:

| approach | 5M edges | 10M edges | delta cost |
|---|---|---|---|
| full recompute (rayon) | 830 MB / 0.84s | 1653 MB / 1.81s | — rebuilds the world |
| Z-set, string `Edge`   | 1872 MB       | **2694 MB**    | tiny 7µs / giant 82µs |
| Z-set, dense `(u32,u32)` (store lab E1) | 881 MB | **1622 MB** | tiny 4–5µs |

Three things this pins down:

1. **Delta cost is flat in corpus size** and tracks the CHANGED FILE, not the 10M
   total: tiny-file delta 7µs at 10M vs 4µs at 1M; giant-file delta 82µs. The 213ms
   full recompute is ~17,000× that. The delta claim holds at scale.
2. **The break is resident memory.** Against the store lab's 1.5GB reference budget,
   the string-`Edge` Z-set overflows before 5M (1872 MB); even the dense `(u32,u32)`
   map overflows before 10M (1622 MB). Past the budget you are ON DISK — the store
   lab's SQL weight cascade — and the delta becomes a SQL UPDATE (its measured
   O(delta·log n) retract), not a hashmap poke. The µs numbers are the resident best
   case; the wall you actually hit is RAM.
3. **Representation is the lever, exactly as the store lab found** (E1 dense i64 key):
   dense `(u32,u32)` is ~40% less RSS than string `Edge` even on this synthetic corpus
   with near-unique symbols; on a real call graph (symbols repeat heavily) the gap is
   larger. This is why the resident-vs-disk boundary moves with the key encoding.

None of these three findings involves streaming rows through rxRust. The core question
is full-recompute vs delta and resident vs on-disk — both answered on the
batch-relational side.

## The one fact everything rotates on

Extraction hands back `Hit<'r>` — a match that BORROWS out of the file buffer (the
ast-grep `Node<'r>` shape, zero-copy). Two consequences, opposite signs:

- In a **batch scope** the borrow lives and dies inside the rayon closure; you
  project to an owned `Edge` at the source and only owned data crosses the reduce
  boundary. The runtime that holds the answer is `struct { edges: BTreeSet<Edge> }`
  — no lifetime, nothing infected.
- On a **stream** the borrow has to ride every downstream operator, and every stream
  tool that buys you concurrency (a spawner, a boxed stream held in the store)
  demands `'static`. The borrow and `'static` cannot both be true.

## Receipt — CORE as a stream fails, mechanically

`cargo build --example core_frp_attempt --features frp-core-attempt`:

```
error[E0521]: borrowed data escapes outside of function
  --> examples/core_frp_attempt.rs:28:9
25 | fn wall_1_spawn_extract(pool: &ThreadPool, files: &[File]) {
   |                                            -----  - `files` is a reference only valid in the function body
28 | /         pool.spawn(async move {
29 | |             let hits: Vec<Hit<'_>> = extract(file);
31 | |         })
   | |__________`files` escapes the function body here
   |            argument requires that `'1` must outlive `'static`
```

**Wall 1 (hard error):** to parallelize per-file extraction — rayon's job — on a
stream scheduler you spawn the work onto a pool; `ThreadPool::spawn` requires
`Future: Send + 'static`; the future borrows the file buffer; `'1` cannot outlive
`'static`. To make it compile you must sever the borrow (own everything first) —
which is the batch you were trying to replace, now done one row at a time. This is
the DataLoader trap the repo's CLAUDE.md warns about, made mechanical.

**Wall 2 (soft cost, compiles):** hold the stream in the store — the session's
"everything is a stream by id" — and `'r` leaks into the runtime type itself:

```rust
struct StreamRuntime<'r> { hits: BoxStream<'r, Hit<'r>> }   // compiles...
```

...but the store can now never outlive any source buffer it ever read, and to do the
actual JOIN/fixpoint you `.collect().await` the whole set anyway — re-batching
inside the stream. The lifetime parameter is pure cost; the stream bought nothing.

## Receipt — EDGE as a stream is clean

`cargo run --example edge`:

```
EDGE trigger — 3 coalesced jobs from the event log:
  Rust  paths=["a.rs", "b.rs"]  head=Some("refs/heads/v11")
  Ts    paths=["ui.ts"]         head=Some("refs/heads/v11")
  Rust  paths=["a.rs"]          head=Some("refs/heads/v11")
```

Every `Event` is owned (`String` paths, `u64` digests, `String` refs), so the whole
graph is `'static + Send`. The pipeline is the proper formula the session called for,
replacing v5's "groupBy + immediate rerun":

```
events
  -> buffer(tick)                  // flush the window on the clock boundary
  -> groupBy(family)               // route by extension, coalesce per family
  -> distinctUntilChanged(digest)  // a re-save with the same content is a no-op
  -> emit DeriveJob                // one job per family per tick — never per row
```

The `a.rs` re-save with an unchanged digest is dropped by the distinct gate; the
later real change (new digest) passes. That is the Docker-layer skip, at the edge,
where it is a dozen lines of owned-event stream operators.

## Verdict

Three positions, not two — the first lab only drew two:

| | what it is | fate |
|---|---|---|
| A. rayon full-recompute family | re-derive the world per edit | **terrible** — O(corpus)/edit (Finding 1). The user was right. |
| B. rxRust/stream the rows | `Hit<'r>` on a scheduler | **impossible/pointless** — E0521 `'static` wall, or a lifetime infecting the store for zero gain |
| C. Z-set / semi-naive IVM | delta over a weight map | **correct and cheap** — 12.2 µs vs 213 ms; the store cascade |

The break point killed A, not C — and C is *more* batch-relational than A, not more
streamed. FRP-core (B) is masochism at any corpus size. FRP-edge stays clean, owned,
`'static` by construction, and is exactly where the operators you want
(buffer/throttle/distinct/groupBy) live.

Landing: the core is **Z-set / semi-naive IVM** (the store cascade), NOT rayon
full-recompute and NOT rxRust row-streams; the trigger is a `futures::Stream`. "Batch
the core" was right about the mechanism (owned, per-tick, not streamed) and wrong to
model that mechanism as full recompute — the delta is the point.
