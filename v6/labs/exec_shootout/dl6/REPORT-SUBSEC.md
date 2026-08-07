# REPORT-SUBSEC: grid_10000 under 1,000 ms in the real engine

Pass 1 of 2. The dl6 entrant (prolog -> TS+SQLite) ran grid_10000 at 1,341 ms
fixpoint after this morning's landings. This lane measured the wall, priced the
structural target, and landed one confirmed TAKE. The sub-1,000 target needs a
head-storage restructure that is a FINDING here, not a landing (rationale at the
bottom).

## TOC
- Opening profile (target list)
- The wall: identical SQLite work, three extra structures
- Race log
- Final grid number with gate table
- Why sub-1,000 requires a restructure (FINDING)
- Reproducing

## Opening profile (unbatched grid_10000, target list)

DL6_BENCH_UNBATCH=1, best-of-2 baseline. The fixpoint is one assert walk over the
transitively-closed reachable: 44-45 rounds, each round = clear wave, hop
(ping/pong JOIN edge, NOT EXISTS reachable), commit (wave -> reachable).

| ms | share | calls | what |
|---|---|---|---|
| 378 | 28.3% | 45 | `INSERT OR IGNORE INTO __ping` (wave hop, NOT EXISTS reachable) |
| 375 | 28.1% | 44 | `INSERT OR IGNORE INTO __pong` (wave hop, NOT EXISTS reachable) |
| 227 | 17.0% | 45 | `INSERT OR IGNORE INTO reachable ... FROM __ping` (commit) |
| 223 | 16.7% | 44 | `INSERT OR IGNORE INTO reachable ... FROM __pong` (commit) |
| 48 | 3.6% | 45 | `INSERT INTO __new_reachable ... FROM __ping` (carry fill) |
| 48 | 3.6% | 44 | `INSERT INTO __new_reachable ... FROM __pong` (carry fill) |

The two `__new_reachable` carry fills are the only rows whose deletion both wins
and keeps the boundary byte-identical. Everything above them is the walk itself.

## The wall: identical SQLite work, three extra structures

The pure-SQLite floor (sqlite_raw, same-session best-of-2: 1001/1023) runs this
closure in ~1,001 ms with ZERO reactive machinery. It writes each derived row ONCE into a bare
rowid+unique table, dedups by OR IGNORE at insert, and tracks the next round's
delta as a ROWID RANGE (no wave table, no NOT EXISTS probe, no carry staging).

dl6 already writes fewer slots than the floor (measured ~3.0 rowsAffected per
derived row across waves+commit, vs the floor's 3.91) BUT it runs slower
because it pays three structure taxes per derived row:
1. a wave write (ping/pong) that the floor does not do,
2. a NOT EXISTS probe against the growing 1M-row reachable on every hop,
3. the carry staging the unobserved-skip keeps anyway.

So the engine's extra work is NOT btree writes, it is the wave + probe + staging
READS AND WRITES the floor's rowid-range delta design eliminates. That is the
whole target list in one line.

## Race log

| idea | hypothesis | result ms (best-of-2, unbatched) | verdict | one line why |
|---|---|---|---|---|
| baseline | landings at 1,341 | 1,376 | - | the number under test |
| skip `__new_<rel>` arrive fills in assert walk | the fill feeds only the carry; commit's rowsAffected carries same signal with one keyed write fewer per round | 1,259 (-117) | TAKE | reachable is unobserved; nothing reads `__new_` in the assert walk, commit rowsAffected is the carry |
| replace walk with one recursive CTE into head | whole closure in one statement | 1,432 (worse) | REJECT | CTE writes WITHOUT ROWID at ~1.0M rows/s, slower than the wave walk |
| replace walk with rowid-range loop, same WITHOUT ROWID head | append delta the way the floor does | 1,123 (bare loop, still >floor) | REJECT (partial) | rowid-range needs a rowid+unique head; on a WITHOUT ROWID head it cannot recover the floor |

## Final grid number with gate table

| gate | value |
|---|---|
| grid_10000 fixpoint (unbatched, best-of-2) | **1,376 -> 1,259 ms** |
| derived | 1,069,200 (both sides) |
| checksum | `9d7239568960d6a8` (both sides) |
| sweep | RUN wrong=0, FINAL wrong=0 (210/211 identical; the 1 rejection is an engine-mandated retract-from-log path, pre-existing) |
| tsgo --noEmit | 0 errors |
| pnpm test | 157 pass, 0 fail |
| plunit (v6/prolog/compile) | 364 (+44 sub) passed |
| chain_10000 / layered_10000 | 26,422/13,057 (baseline) -> 23,882/12,112 (candidate): IMPROVED, no regression |

chain and layered shared the same skipped-rel assert walk, so the candidate
improves them rather than regressing them (checked against the parent commit).

## Why sub-1,000 requires a restructure (FINDING)

The mission's framing is exact: "subsecond means the full reactive engine does
LESS SQLite work than that minimal script." The landing in 1,259 ms moves the
engine toward that, but the remaining ~260 ms over the ~1,001 ms floor comes from
the walk's three structures (wave table, NOT EXISTS probe, carry staging) which
are what keep the sweep byte-identical for arbitrary programs. The floor's
sub-second comes from a design the engine does not and cannot bolt on locally:

- rowid-range delta requires the head to be a rowid+unique table (append + range
  read), but dl6 emits EVERY rel head as WITHOUT ROWID (set semantics, PK is the
  row locator, `lower.pl`). Changing the head shape is a compiler change that
  ripples into the emitted DDL, the delta/frontier/merge column lists, and the
  boundary read of EVERY program, not just this bench.
- dropping the wave + NOT EXISTS probe changes semi-naive evaluation order, which
  the reconciliation sweep asserts byte-for-byte.

So a green sub-1,000 on grid_10000 in the real engine is not a statement-level
deletion, it is a head-storage and walk restructure that must be validated across
the whole compiler, not this one test. That is the review pass's question. The
landed TAKE is safe (unobserved-skip only, row-count identical, all gates green)
and is the step that does not touch the general-evaluation contract.

## Reproducing

```
cd v6/labs/exec_shootout/dl6
DL6_BENCH_UNBATCH=1 bash bench.sh   # best-of-2, checksum 9d7239568960d6a8, derived 1,069,200
cd v6/tsv2 && bash scripts/sweep.sh
cd v6/prolog/compile && swipl -f none -g "load_files(['test/plunit_tests.pl'], []), run_tests." -t halt
```

FACTS.md is rewritten by bench.sh; restore it with `git checkout -- FACTS.md` after runs.
