---
name: sqlite-costs
description: The measured cost constants of SQLite on this machine — btree write rates by key shape, what an index really is, which optimizations are already-disproven losers, and where time physically goes in a keyed insert. Read BEFORE proposing any SQLite performance change, schema experiment, or "just batch it" idea in any lane or lab.
---

# SQLite cost constants (measured, labs/exec_shootout, 2026-08-06/07)

Apple M2 Pro, :memory:, libsql 0.5.29 / rusqlite bundled. Sources:
sqlite_raw/REPORT.md, REPORT-BATCH.md, REPORT-TAIL.md,
intern_bench/REPORT-INTERN.md, dl6/FACTS*.md,
head_shape/ (2026-08-08, the head-shape A/B on the post-intern flagship).

## Write rates by structure (the ladder that decides designs)
| structure | rate | note |
|---|---|---|
| bare rowid append, no index | ~10M rows/s | the medium is fast without keys |
| rowid table + UNIQUE index | ~1.34M rows/s | the semi-naive dedup floor |
| 4-col WITHOUT ROWID PK, INTEGER | ~3.3M -> 2.9M rows/s (10k -> 1M rows) | decays as tree deepens |
| 4-col WITHOUT ROWID PK, TEXT | ~1.9M -> 1.5M rows/s | 1.7-2.0x slower than INTEGER, always |
| rust FxHashSet insert | ~68M rows/s | the 50x the medium cannot close |

## Facts that veto common proposals
- Statement dispatch is free: 2,582 in-process dispatches cost 4 ms total.
  Batching/fusing statements recovers nothing; only deleting WORK counts.
- An index IS a copy of its key. One-table-with-state-columns vs many tables
  measured as a wash (dl6/onetable probe); you save a write only by deleting
  the QUESTION it answers, never by relocating the answer.
- A statement that reads its own INSERT target forces an ephemeral snapshot:
  +1 transient write per candidate row (EXPLAIN: OpenEphemeral).
- OR IGNORE rejection BEATS a NOT EXISTS prefilter on identical storage at
  every duplication rate measured (1.4x). The old opposite reading came from
  a different plan shape (delta staging LEFT JOIN).
- ORDER BY on the insert's SELECT (sorted-append theory): measured loser.
- Double-hop round unrolling: 2.4x loser; cost tracks join candidates, never
  round count.
- Packed single-INTEGER keys vs two INT columns: wash on pure insert
  (6,565 vs 6,777 ms / 10M rows); btree page work dominates key width.
- Pragmas on :memory: are no-ops (journal/sync/cache); page_size=16384 is the
  one real effect (~100 MB RSS on 10M rows).
- WITHOUT ROWID BEATS rowid+UNIQUE on a fixpoint head, both sides measured on
  the SAME algorithm: rowid+UNIQUE is 5.4-7.6% SLOWER and 2.4x fatter (35.5 vs
  15.0 MB grid, 360 vs 148 MB chain), because it stores every key twice, table
  plus index. True with TEXT keys and still true with INTEGER keys after
  interning, so the dictionary flip does not move this call. The old "16%
  slower fixpoint, 2.2x less memory" line compared two different ALGORITHMS,
  not two storage shapes: a rowid-range delta against a ping/pong wavefront.
  The rowid-range delta is worth 17-53%, and it is the DELTA that requires a
  rowid, never the storage on its own.
- Interned INTEGER keys vs raw TEXT keys, whole fixpoint on the 4-column
  flagship head: 1.69-1.94x faster, 9.0-9.4x smaller on disk. The 1.7-2.0x in
  the ladder above is the insert alone; the fixpoint holds the same band.
- Recursive CTE vs statement loop: loop wins wide frontiers ~1.3x, loses on
  deep-thin chains; shape-dependent, both banked.
- MEASURE THE SEAM'S SHARE FIRST. On the v6 Rust engine
  (`sprefa-engine-rs`), SQLite was 11.9% of the wall on the sf_join 54k-row /
  6-tick fold; 88% was Rust, in five dedup loops that scanned what they had
  already collected. `DL_TRACE_SUMMARY=1` prints the per (verb, relation) split;
  read it before proposing any SQL change. Same fold after the Rust fix:
  5441 -> 1009 ms, and only then is the seam 68% of the wall.
- `prepare_cached` + `set_prepared_statement_cache_capacity(IR statement count)`
  on that seam: sf_join 996/984 ms cached vs 1004/1062/1005 uncached; dead-module
  rail 1283/1240/1218 vs 1270/1226/1236. 2-5% on one workload, a wash on the
  other. KEPT, and it confirms rather than overturns "dispatch is free": the
  compile is not where the time is.
- Pragmas on `:memory:` again, this time on the engine's own fold:
  `page_size=16384` + `temp_store=MEMORY` measured 988/966 ms against
  991/987/1003 without. No change outside noise.
- FILE-backed (`DL_DB_URL`) sf_join, `journal_mode=OFF` + `synchronous=OFF` +
  `cache_size=-262144` + `mmap_size=1G`: 992/972 ms against 1014/1015/1012
  without. ~2%, and the file-backed fold is otherwise indistinguishable from
  `:memory:` (byte-identical tick log, 4 MB database).
- One transaction per TICK on the FILE-backed path: 992/972 ms with, 984/993/998
  without. A wash, so REJECTED on the file path too, not only in memory. With
  `journal_mode=OFF` there is no journal for a transaction to amortize.
- `SQLITE_LIMIT_VARIABLE_NUMBER` read from the connection on the bundled build is
  32766, so the hand-written 30000-placeholder arrival budget was already the
  right number and nothing moved when the code started asking. Do not set
  `SQLITE_MAX_VARIABLE_NUMBER` in build env: bigger IN-lists measured SLOWER
  (the run-grouping row above).

## Where a keyed insert's time goes (mechanism)
Binary-search the btree path (every compare on TEXT walks the keys' shared
prefix bytes), touch log_fanout(N) pages, insert into the leaf, split pages on
overflow copying keys again. Fat keys cut fanout, deepen the tree, and pay
prefix memcmp per compare — that is the whole TEXT tax; see the
sql-relational-design skill for the law it forces.

## The engine-level decomposition (chain_10000 cold build)
Per-statement profile lives in dl6/FACTS.unbatched.md; the tail alone (fill +
keyed head insert) is 7.6 s of any design's budget (REPORT-TAIL.md), and the
head insert is 89% of that tail. Anything claiming a big win must name which
of those statements it deletes.
