# v6 cascade tuning log

Each entry: hypothesis, change, measurement, verdict (KEEP / RETRACT). Numbers are
from `cargo run --release --example sqlite_reach -- 10 500000` (5,000,002 nodes /
9,666,667 edges / 500,000 killed / depth 3), `DL_MEMCAP_MB=0`, on-disk WAL. Retract
is the MEASURED op; setup is one-time and not the headline.

Resident targets for reference (same graph, 1.5GB budget): dd/dbsp retract ~290ms
but ABORT past the memory wall where sqlite completes.

## E0 — baseline (composite WITHOUT ROWID key)
- Schema: `cx_row(tag,id,weight) PK(tag,id) WITHOUT ROWID`; `cx_dep` 4-tuple PK WITHOUT ROWID.
- 3 runs: retract 2.579 / 2.564 / 2.579 s. Setup ~8.0s. peak_rss ~1.35-1.46 GB.
- 29 stmts, 3 rounds. **This is the number every experiment below is measured against.**
- db on disk (WAL folded): 224.7 MB.

## E1 — single dense-int key (rowid table) — KEEP
- Hypothesis: `(tag,id)` composite WITHOUT ROWID pays full-key bytes in every
  b-tree node + comparison. Encode to one dense i64; make `cx_row` a ROWID table
  clustered on `key INTEGER PRIMARY KEY` (rowid = free, native fastest lookup) and
  `cx_dep` a 2-column `(parent_key,child_key)` instead of a 4-column composite.
- Change: src/cascade.rs schema + all retract SQL to single-key; tag/id kept as
  plain output columns on cx_row; equivalence oracle rewritten to key space.
- Measure (3 runs): retract **1.505 / 1.516 / 1.494 s** (was 2.57) — **−42%**.
  Setup 8.0 -> 7.5s. peak_rss ~1.48 GB (flat). db **230.8 MB** (was 224.7, **+2.7%**).
- Verdict: KEEP. Time −42% is the axis we chase (dd/dbsp ~291ms; ~9x -> ~5x gap).
  Space +6MB is the redundant tag/id on cx_row (key already encodes them) — E2 target.
- Guard: full suite green incl. head_to_head 4-engine byte-identical.

## E2 — drop redundant tag/id, make them VIRTUAL generated columns — KEEP
- Hypothesis: E1 stored tag+id as payload AND key=encode(tag,id) as rowid — a
  redundant copy (the +2.7% disk). Replace with `tag/id GENERATED ALWAYS AS
  (key/1e9), (key%1e9) VIRTUAL` — computed on read, zero storage — so cx_row
  payload is just weight. Every `WHERE tag=.. AND id=..` assertion still resolves.
- Change: src/cascade.rs schema (2 generated virtual cols) + insert into (key,weight).
- Measure (3 runs): retract **1.488 / 1.473 / 1.474 s** (flat vs E1). db **207.0 MB**
  (E1 230.8, E0 224.7 — **−7.9% vs baseline**, −10.3% vs E1). Setup 7.5 -> 6.9s.
- Verdict: KEEP. Space reclaimed at zero time cost; VIRTUAL cols cost nothing to store.
- Cumulative E0 -> E2: retract **2.57 -> 1.48 s (−42%)**, db **224.7 -> 207.0 MB (−7.9%)**,
  setup 8.0 -> 6.9s (−14%). Both axes down.
- Guard: full sprefa-store suite green incl. head_to_head 4-engine byte-identical.

## E3 — mmap_size=512MB for retract reads — REJECTED
- Hypothesis: random PK lookups into cx_row/cx_dep are disk-read-bound; mmap the
  whole 207MB db so reads skip the heap-cache copy.
- Measure: retract 1.455 / 1.454 (mmap 512) vs 1.437 / 1.442 (mmap 0). No change
  (slightly worse, within noise).
- Verdict: REJECTED, reverted. The 207MB db already sits in the OS page cache after
  setup, so retract is NOT read-bound. The remaining cost is CPU in SQLite's VDBE
  (GROUP BY / UPDATE over 100k-row wavefronts), not disk. mmap is the wrong lever.

## Per-statement breakdown (DL_CASCADE_TRACE=1, added to cascade.rs) — the map
At 5M/500k/depth-3, the retract's ~1.45s splits (summed over 3 rounds):
- weight UPDATE (correlated subquery): 212+227+192 = **631 ms** (dominant)
- cx_hits build (CROSS JOIN cx_dep + GROUP BY child_key): 161+258+130 = **549 ms**
- cx_next (transition guard): 66+40+28 = 134 ms
- cx_frontier copy: 44+20+0 = 64 ms
- everything else (DELETEs, seed, counts): < 10 ms total
The UPDATE is ~1M scattered rowid row-rewrites into the 5M-row cx_row, each
WAL-logged — the on-disk penalty dd/dbsp avoid (RAM hashmaps, no page writes).

## E4 — UPDATE..FROM cx_hits instead of correlated subquery — REJECTED
- Hypothesis: one join-driven update beats a correlated subquery + IN(SELECT).
- Measure: retract **2.52s** (was 1.48) — 70% WORSE.
- Verdict: REJECTED, reverted. Without the `WHERE key IN (SELECT..)` the planner
  drove the join from the 5M-row cx_row (full scan x3 rounds) instead of the tiny
  cx_hits. The IN(SELECT) was doing load-bearing join-order pinning. Correctness
  held (tests green) — pure perf regression.

## E5 — bigger page cache (cache_size 32 -> 512 MB) — REJECTED
- Hypothesis: the UPDATE spills dirty pages past the 32MB cache mid-transaction.
- Measure: retract 1.436 (32) / 1.439 (128) / 1.421 (256) / 1.416 (512) MB cache.
  ~1.4% gain for 16x memory. peak_rss 1602 -> 1651 MB.
- Verdict: REJECTED, reverted. The dirty-page set fits in 32MB; retract is NOT
  cache-spill bound. Answers the open tradeoff question: relaxing the memory bound
  buys ~nothing here. 32MB stays optimal and the bounded-memory thesis holds.
