---
name: sqlite-costs
description: The measured cost constants of SQLite on this machine — btree write rates by key shape, what an index really is, which optimizations are already-disproven losers, and where time physically goes in a keyed insert. Read BEFORE proposing any SQLite performance change, schema experiment, or "just batch it" idea in any lane or lab.
---

# SQLite cost constants (measured, labs/exec_shootout, 2026-08-06/07)

Apple M2 Pro, :memory:, libsql 0.5.29 / rusqlite bundled. Sources:
sqlite_raw/REPORT.md, REPORT-BATCH.md, REPORT-TAIL.md,
intern_bench/REPORT-INTERN.md, dl6/FACTS*.md.

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
- WITHOUT ROWID vs rowid+unique: 16% slower fixpoint, 2.2x less memory
  (pairs stored once, not table+index).
- Recursive CTE vs statement loop: loop wins wide frontiers ~1.3x, loses on
  deep-thin chains; shape-dependent, both banked.

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
