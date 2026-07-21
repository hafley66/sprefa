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
