# In-place recursive-head maintenance: banked numbers

Landing receipt for `IDredPlan` (lane `lane/dred-in-place`). `FACTS.md` and
`FACTS.unbatched.md` are untouched; everything below was measured on one
machine in one session, `14c03007` (refCount recompute) against
`ccfeabcc` (in-place assert/DRed), regenerating `gen_emitted/` between runs.

Node v24.15.0 · darwin/arm64 · 2026-08-06.

## TOC
1. What moved: incremental ticks
2. What did not move: the single build tick
3. The one case that got slower
4. Regenerate

## 1. Incremental ticks — `incbench.ts`, grid 45x45, head 1,069,200 rows

```
node --experimental-transform-types incbench.ts "$PWD/.compiled/reachability.ts" 45
```

| tick | refCount recompute | in-place | ratio |
|---|---|---|---|
| build the closure (1.07M rows) | 2,094 ms | 2,166 ms | 0.97x |
| insert one edge, no new rows | 2,019 ms | 42 ms | 48x |
| delete one edge, no rows lost | 3,907 ms | 56 ms | 70x |
| delete a structural edge (-44 rows) | 3,878 ms | 82 ms | 47x |
| empty drain tick | 1,926 ms | 1 ms | 1,926x |

A retraction tick reconciles TWICE (`applyLevelsBeforeEdges` for the frozen
mid-tick closure, `recomputeLevelsAfterEdges` after the edge writes), which is
why every delete row above is about double its insert twin on the refCount
side.

## 2. The single build tick — `bench.sh`, min of 2 runs each

The three `bench.sh` cases feed every edge in ONE tick, so they measure a
build from an empty head: the one shape where the refCount path is already at
its floor (two writes per round, and its whole tail — the head UPDATE, the
antijoin, the bulk head insert — runs against an EMPTY head).

| case | refCount recompute | in-place | delta | peak RSS |
|---|---|---|---|---|
| `grid_10000` | 2,110 ms | 2,141 ms | +1.5% | 740 -> 697 MB |
| `layered_10000` | 19,954 ms | 20,386 ms | +2.2% | 2,155 -> 2,078 MB |
| `chain_10000` | 32,112 ms | 33,338 ms | +3.8% | 2,286 -> 2,078 MB |

Checksums identical in every case. `chain_10000` swings 33-56s run to run at
10M rows on this machine, so treat its delta as noise-band, not signal.

Why there is no drop here, measured rather than argued (scratch probe, grid
50x50 and chain 2000, three walk shapes timed separately): a row written
DURING the walk costs ~1.9 us and a row written in a bulk tail statement
~0.31 us, so the walk dominates and both paths already write two rows per
derived row inside it. The refCount path writes wavefront + `__support_next`;
the in-place path writes wavefront + head + `__new_<rel>`, and pays its third
walk write back by dropping the tail's head UPDATE, antijoin and bulk insert.
It is a wash by construction, and `FACTS.unbatched.md`'s "absorb is 53% of
chain" is the accumulator write, which no shape avoids.

## 3. Deleting 100 scattered edges at once

| tick | refCount recompute | in-place | ratio |
|---|---|---|---|
| insert 100 random jumps (+409k rows) | 2,919 ms | 3,338 ms | 0.87x |
| delete those 100 (-409k rows) | 4,469 ms | 11,007 ms | 0.41x |

The retraction cone here is 409k of a 1.48M head = 28%, just past the
`head/4` bail, so the walk runs nearly to the cap and then rebuilds anyway —
and a retraction tick does that TWICE. Spec 2026-08-06 §3 names this worst
case ("rebuild + bounded walk"); what it does not price is the second pass.
Break-even from the numbers above is cone ~= head/12 (a cone row costs ~6x a
rebuild row, and DRed walks the cone twice), so a tighter cap is the obvious
follow-up. The cap stays `head/4` here because that is the raced, forced
policy.

## 4. Regenerate

```
cd v6/labs/exec_shootout/dl6
DL6_BENCH_FULL=1 bash bench.sh                      # sections 2
bash bench.sh                                       # rebuild .compiled only
node --experimental-transform-types incbench.ts "$PWD/.compiled/reachability.ts" 45
```
