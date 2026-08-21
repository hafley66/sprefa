# v6 core: the lab arc — oracles, why, efficiency, measured Big-O + empirical ratios

Date: 2026-07-21. Three standalone labs built and run this session, each answering one
fork with running code and an **independent oracle**, not assertion. This is the record.

- `v6/frp-lab`      — FRP-core vs batch-core; then the batch break point (delta vs recompute).
- `v6/reactor-lab`  — salsa on the real crate: behavior + is-it-resident.
- `v6/temporal-lab` — append-only bitemporal on SQLite + retraction, cross-checked at scale.

Prior, already landed: `v6/sprefa-store` (EXPERIMENTS.md) — the on-disk weight cascade
(E0–E7), the recursive-retraction kernel these labs point back to.

---

## The settled architecture (what the arc converged on)

```
EDGE  (event streams)  → futures::Stream trigger: buffer→groupBy→distinct→DeriveJob
                         owned, 'static, clean.  FRP earns its keep HERE only.
CORE  (batch-relational)
  Layer 2  CONTROL   = salsa (red-green). Memo = DIGEST + revisions + deps.
                       RESIDENT, O(rels), KB. Decides WHICH rel is dirty.
  Layer 1  FACTS     = append-only bitemporal table on SQLite. Retract = close interval.
                       ON DISK, O(facts). The Z-set/semi-naive cascade is this, weighted.
```

One monotonic `revision` counter serves dirty-check (salsa), retraction (weight→0), and
temporal as-of (interval close) — they are the same event stamped by the same clock.
SQLite's WAL already is a monotonic append-only multi-version log; we retain what it
checkpoints away. Single binary, zero new runtime deps for the core.

---

## Master table — each algorithm: why, efficient-because, Big-O, MEASURED, oracle

| # | algorithm / decision | why we need it | efficient because | Big-O | measured (empirical) | independent oracle | verdict |
|---|---|---|---|---|---|---|---|
| 1 | **rayon full-recompute family** | baseline: derive rels from source | rayon over `&[File]` slab; owned at source | **O(corpus) / edit** | 1-file edit over 100k files = **213 ms**, = full build | differential (batch = itself) | ✗ terrible per-edit |
| 2 | **rxRust / stream the rows** (FRP core) | considered: stream the fact rows | — | — | **does not compile** (E0521: `'static` wall on `Hit<'r>`) | **the rustc compiler** | ✗ infeasible |
| 3 | **Z-set delta** (non-recursive) | incremental derive without recompute | net delta over a weight map; index-driven | **O(Δ)**, flat in corpus | 1-file delta over 100k = **12.2 µs** → **~17,000× vs recompute**; flat 7µs@1M…7µs@10M | differential oracle, **20,000 states** (50 seeds×400 steps) | ✓ correct + cheap |
| 4 | **naive plain-set delta** | tempting shortcut | — | — | **wrong**: drops a fact a 2nd file still asserts | same oracle **caught it** (seed 0, step 11) | ✗ (proves oracle has teeth) |
| 5 | **Z-set resident memory** | can the delta live in RAM? | — | **O(facts)** | string Edge **252 B/edge**; 10M = **2694 MB**; dense (u32,u32) = **1622 MB** (−40%) | getrusage, one process/scenario | RAM-bounded: overflows 1.5GB by ~6M |
| 6 | **salsa behavior** (red-green) | dirty-check control plane | recompute only the dirty subgraph; backdating cuts waves | O(dirty subgraph)/edit | cold 4 exec; no-change **0 exec**; 1-file edit **2 exec**; same-digest edit **1 exec, downstream cut** | salsa's own **event log** (WillExecute/DidValidate) | ✓ right tool for control |
| 7 | **salsa residency: rows vs digest** | does salsa eat the budget? | digest memo is 8 B; rows memo is O(facts) | rows **O(facts)** / digest **O(rels)** | same 10M facts: rows **78.5 MB**, digest **0.4 MB** → **~200×**, gap grows with facts | getrusage, two strategies | memoize DIGEST, not rows |
| 8 | **recursive retraction (semi-naive/DRed)** | transitive rules; delete-propagation | frontier→hits→next; GROUP BY child count dead parents | **O(Δ·log n)** | retract 2.57→**1.48 s** (E0→E2, −42%) on 5M/9.6M/500k-killed | 4-engine byte-identical (sqlite/dd/dbsp/naive) | ✓ (store lab) |
| 9 | **append-only bitemporal + close-on-retract** | durable + temporal + retraction in one write | set-based commit (JSON batch → UPDATE over partial live index → close); no N+1 | commit **O(Δ·log n)**; RSS **O(working set)** | 3M facts / 150 revs = **+20 MB RSS** (on disk, not resident) | SQLite live-set == RAM oracle == **salsa**, 2000 revisions | ✓ correct + bounded |
| 10 | **retention compaction** | stop append-only growth (rotation) | one `DELETE WHERE tt_to≤horizon` + VACUUM; live untouched | O(dead dropped) + VACUUM | churn 800k dead → deleted **780k**; rows 3.8M→**3.02M**; **live digest byte-identical** | live-set digest before==after | ✓ live-safe, size-bounded |
| 11 | **bitemporal cross-rev fact** | `moved(revA→revB)`, `--move` | 2 revs in the KEY (valid-time); 1 tt-interval | same as (9) | present as-of birth rev, absent as-of now; history retained | as-of assertions | ✓ two times supported |

---

## Why each oracle is INDEPENDENT (the fact-check, not self-marking)

The point the user pressed: a system cannot grade its own homework. Each claim above was
checked against a mechanism that does not share the code under test:

- **differential oracle** (rows 3,4): oracle is a from-scratch `derive_family_batch`
  recompute — a separate, dead-simple implementation. The incremental engine must equal it
  after *every* edit in a random stream. It caught the naive delta at seed 0 step 11
  without any hand-crafted case → the oracle has teeth, so passing means something.
- **the rustc compiler** (row 2): the most independent oracle available. FRP-core is not
  "hard," it is `error[E0521]`. Not an opinion.
- **salsa's own event log** (row 6): `WillExecute` vs `DidValidateMemoizedValue` are emitted
  by the framework, independent of our counting. The EXECUTE count *is* the recompute-work
  metric (how rust-analyzer profiles).
- **getrusage / peak RSS** (rows 5,7,9): the OS kernel's accounting, not ours. One
  scenario per process so peak is uncontaminated (a first cut mis-attributed a
  full-recompute's 1.7 GB to the Z-set — caught and fixed by process isolation).
- **triple cross-check** (row 9): SQLite `==` RAM multiset `==` salsa, at every checkpoint
  over 2000 revisions. Three independent implementations of "the live set" must agree.
- **self-consistency digest** (row 10): compaction is safe iff the live-set XOR digest is
  byte-identical before and after. Any drift fails the assert.

---

## Per-lab measured tables

### frp-lab — full recompute vs delta

| corpus | full build | 1-file edit, full recompute | 1-file edit, Z-set delta |
|---|---|---|---|
| 1k  | 2.32 ms | 2.47 ms | — |
| 10k | 23.8 ms | 19.2 ms | — |
| 100k| 194 ms  | **213 ms** | **12.2 µs** (~17,000×) |

### frp-lab — resident memory at scale (per process, skewed families)

| approach | 5M edges | 10M edges | delta cost |
|---|---|---|---|
| full recompute (rayon) | 830 MB / 0.84 s | 1653 MB / 1.81 s | rebuilds world |
| Z-set, string `Edge`   | 1872 MB | **2694 MB** (252 B/edge) | tiny 7 µs / giant 82 µs |
| Z-set, dense `(u32,u32)` | 881 MB | **1622 MB** (−40%) | tiny 4–5 µs |

### reactor-lab — salsa residency (same 10M facts, only return type differs)

| total facts | `rows` → `Vec<u64>` | `digest` → `u64` |
|---|---|---|
| 1M | 9.6 MB | 1.7 MB |
| 5M | 48.6 MB | 1.7 MB |
| 10M | **79.9 MB** (memo +78.5) | **1.7 MB** (memo +0.4) → ~200× |

### reactor-lab — salsa behavior (EXECUTE = recompute work)

| step | EXECUTE | VALIDATE |
|---|---|---|
| cold | 4 | 0 |
| re-query, no change | **0** | 0 |
| edit b (1→3 edges) | 2 | 2 |
| edit a, **same** digest | 1 | 3 (early cutoff: `total_edges` validated, not executed) |

### temporal-lab — the SQLite bitemporal engine

| phase | result |
|---|---|
| 1 correctness | SQLite == RAM oracle == salsa, 2000 revisions, all checkpoints ✓ |
| 2 cross-rev | `moved(rev5→rev7)` present as-of birth, absent as-of now; history kept ✓ |
| 3 scale | 3M live facts / 150 set-based revisions → **peak RSS 31 MB** (base 11) |
| 4 compaction | 800k dead → deleted 780k; rows 3.8M→3.02M; **live digest UNCHANGED** ✓ |

---

## Big-O summary (the shape, independent of constants)

| layer | operation | Big-O |
|---|---|---|
| control (salsa) | dirty-check per edit | O(dirty subgraph); resident O(rels) |
| facts (Z-set delta, non-recursive) | per edit | O(Δ), resident O(facts) |
| facts (recursive retraction) | per edit | O(Δ·log n) |
| facts (temporal commit, SQLite) | per revision | O(Δ·log n) writes; RSS O(working set), NOT O(facts) |
| compaction | per pass | O(dead rows) + VACUUM O(db) |
| full recompute (rejected) | per edit | O(corpus) |

The through-line: everything the core does per change is **O(Δ)** or **O(Δ·log n)** — bounded
by the *change*, not the corpus — and the fact residency is pushed to disk so RSS is bounded
by the working set, not the fact count. That is why single-binary + SQLite scales here.

---

## What is proven vs what remains

Proven (running, oracle-checked): the edge/core split; batch-core over FRP-core (compiler);
delta over recompute (17,000×, 20k-state oracle); Z-set weight = the correct delta; salsa is
resident and only cheap when it memoizes digests (200×); recursive retraction O(Δ·log n)
(store lab); append-only bitemporal on SQLite correct vs salsa at 2000 revisions; RSS bounded
at 3M facts; compaction live-safe; cross-rev/bitemporal facts.

Remaining (not yet labbed): the reactor's dirty-set differential oracle (salsa's re-run set ==
the set that actually changed) at large rel-count; addition-cascade direction for recursive
rules; generalize `cx_*`/`fact` schema from single-i64 key to arbitrary rels; wire the
futures::Stream trigger to the reactor.

---

## Addendum — `v6/labkit`: the golden harness + the gun (2026-07-21 late)

Traitified. `Experiment` is one trait; `Harness` sweeps scales, counts everything, snapshots
the query plan, fits the empirical Big-O (log-log slope) and checks it against the DECLARED
complexity, and cross-checks every engine's digest for equivalence. Answer to "can each
experiment be forced to carry these?": yes — declare Big-O, harness falsifies it against the
measured slope (Big-O can't be derived from one run, only inferred from the sweep).

Golden table (file-backed SQLite, 5 GB gun, scales 100k→3M, 200 ticks × 100 edits), all
engines digest-equivalent at every scale:

| engine | declared time | **measured t^p** | apply_ms @3M | writes | recompute |
|---|---|---|---|---|---|
| ram-zset | O(Δ) | ~0 (flat) | 1.0 | 16585 (O(Δ)) | 0 |
| sqlite-temporal | O(Δ·log n) | 0.18 | 437 | **1300 (~5/tick, flat)** | 0 |
| salsa-rows | O(facts)/tick | **1.07 (linear)** | **8951** | 0 | 201 |

- salsa-in-fact-role is now a **measured 1.07 slope**, not a claim — the "salsa for control,
  not facts" split, quantified.
- N+1 tripwire: sqlite ~5 statements/tick independent of Δ and scale (set-based).
- The harness FALSIFIED sqlite's declared `O(working set)` space (in-process slope ~0.88);
  the clean per-process number (temporal-lab, 31 MB @ 3M file-backed) is the real bound — a
  single shared process can't isolate memory, and the harness surfacing the gap is the point.

The gun: `gun::Gun` = `#[global_allocator]` ported from `sprefa-store::memcap::CappedAlloc`
(null past cap → SIGABRT). `install(5120)` = 5 GB Rust cap + Linux setrlimit belt. SQLite C
heap is invisible to it (store's RAM audit) → sqlite experiment adds `soft_heap_limit`.

Slots ready, unfilled: dd/DBSP experiments (port store's `dd_reach`; `with-dd` feature wired);
a recursive-rule workload using the store's semi-naive DRed cascade (`recompute_units` =
fixpoint rounds) — the retraction techniques already solved in `sprefa-store/src/cascade.rs`.
