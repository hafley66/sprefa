# REPORT-BATCH: can batching or storage tricks beat the sqlite floor?

Five experiments racing the `loop_range_rowid` baseline on the batching/storage
axes, inside SQLite, single-threaded, in-memory, `chosen` pragmas. All rows
reproduce the banked derived counts and checksums exactly (ONE btree did not
break the fold). Machine: darwin/arm64, node v24.15.0, libsql 0.5.29.

## Setup note

The brief's banked inputs at `dl6/.bench/*.in` were missing from the tree and
from git. `harness --engines ref --scales 10000` regenerates them
deterministically (grid/chain structural, layered seeded); every regenerated
file reproduces its banked checksum and derived count under the baseline, so the
recovered files are faithful. Same-session baselines below replace the banked
numbers for every delta (machine drift).

## The gap being attacked

| | grid_10000 | chain_10000 | layered_10000 |
|---|---|---|---|
| banked best fixpoint ms | 992-1068 | 9,212-9,798 | 9,559-10,797 |
| **same-session best (this lane)** | **1,015** | **8,582** | **9,437** |
| pure keyed insert rate anchor | 1.34M rows/s | 1.34M rows/s | 1.34M rows/s |

## Results table

All `delta` values are vs the same-session baseline rerun of `loop_range_rowid`.
`E1` reports dispatch ms for 2,582 no-op `.run()` calls and one `db.exec()` of
the same statements; it has no fixpoint.

| exp | case | fixpoint ms | delta vs baseline | rounds | checksum |
|---|---|---|---|---|---|
| baseline | grid | 1,015 | 0 | 87 | MATCH |
| baseline | chain | 8,582 | 0 | 2,580 | MATCH |
| baseline | layered | 9,437 | 0 | 191 | MATCH |
| E1 dispatch | grid | 4 | n/a | - | - |
| E1 dispatch | chain | 4 | n/a | - | - |
| E1 dispatch | layered | 3 | n/a | - | - |
| E1 fused exec | grid | 5 | n/a | - | - |
| E1 fused exec | chain | 5 | n/a | - | - |
| E1 fused exec | layered | 5 | n/a | - | - |
| E2 double-hop | grid | 2,642 | +1,627 | 44 | MATCH |
| E2 double-hop | chain | 20,386 | +11,804 | 1,290 | MATCH |
| E2 double-hop | layered | 24,324 | +14,887 | 96 | MATCH |
| E3 packed key | grid | 1,015 | 0 | 87 | MATCH |
| E3 packed key | chain | 13,893 | +5,311 | 2,580 | MATCH |
| E3 packed key | layered | 10,605 | +1,168 | 191 | MATCH |
| E4a ordered rowid | grid | 1,293 | +278 | 87 | MATCH |
| E4a ordered rowid | chain | 11,565 | +2,983 | 2,580 | MATCH |
| E4a ordered rowid | layered | 12,934 | +3,497 | 191 | MATCH |
| E4b ordered packed | grid | 1,267 | +252 | 87 | MATCH |
| E4b ordered packed | chain | 13,992 | +5,410 | 2,580 | MATCH |
| E4b ordered packed | layered | 12,661 | +3,224 | 191 | MATCH |
| E5 combination | grid | 2,869 | +1,854 | 44 | MATCH |
| E5 combination | chain | 16,905 | +8,323 | 1,290 | MATCH |
| E5 combination | layered | 27,346 | +17,909 | 96 | MATCH |

E3/E4 rows are single runs except E3 grid, which ran 1,015/1,020 best-of-2
because it was the only cell within 15% of its baseline. Baselines are best-of-2.

## E1 dispatch-cost bound and statement fusion

The bound is 3-4 ms to dispatch 2,582 prepared `.run()` calls, and 5 ms to run
the same statements fused into one `db.exec()`. Both are far under the 100 ms
threshold the brief named, so the ceiling of ALL dispatch batching is 4 ms on
this workload's statement count. Statement fusion (jamming rounds into
multi-statement text) does not help: the fused path is marginally slower,
because `:memory:` dispatch is already a native call into libsql with no I/O to
amortize. Dispatch is not on the critical path; the fixpoint cost is the btree
insert work inside each statement. This experiment's negative result is the 
whole answer to the thesis: batching per se has nothing to recover.

## E2 double-hop unroll

One round derives two hops (single-hop plus double-hop, both scoped to the same
rowid range) and halves the round count exactly: chain 2,580 to 1,290, grid 87
to 44, layered 191 to 96. The frontier bookkeeping that makes this correct: the
delta stays a rowid range `[low, high]`; run the single-hop statement, then the
double-hop statement against the SAME `[low, high]` (both must see round-start
state, so range bookkeeping must not advance between them), and advance
`low = high+1; high += c1 + c2`. The double-hop alone loses odd-length paths,
which is why the two-hop pair is mandatory. All three checksums MATCH, so the
bookkeeping is right. But halving rounds does nothing for wall time: chain is
2.4x slower than baseline (20.4 s vs 8.6 s). The reason is visible in the join:
the double-hop statement joins `edge` twice per candidate, so each round does
more join work, and the `OpenEphemeral` snapshot now materializes a larger
spill per round. The fixpoint cost is proportional to join candidate work plus
btree inserts, not to the number of statements. Splitting a round in half does
not split the work.

## E3 packed single-integer key

`reachable(pair INTEGER PRIMARY KEY)`, pair = source*2^32 + target (nodes
asserted < 2^32 on load), so one btree is storage plus dedup and keys compare as
single integers. The rowid-range delta trick cannot survive: rowid equals pair,
which sorts by value, not by insertion order. I chose a pong/ping wave-frontier
pair of small `pair` tables (each round's step writes the survivors into the
empty frontier and promotes them), rather than a single spill table, because a
single frontier would need a truncate between read and write in the same round
and the wave avoids that without a second index probe. E3 grid ties the baseline
(1,015 vs 1,015) and is the single best experiment on grid, but chain is +5.3 s
and layered +1.2 s. The single-key PK buys real dedup for the frontier wave, but
the wave introduces per-row overhead the rowid append does not pay: every pair
is written twice (once into the frontier, once into `reachable` on promote via
`INSERT OR IGNORE`), which is the extra insert cost REPORTS's writes-per-row
column anticipated. On chain (0% duplicates) that is pure waste.

## E4 sorted insert order

Adding `ORDER BY known.source, edge.target` to the hop so index inserts arrive
sorted (right-edge appends) does not help; it is slower on every case (E4a grid
+278, chain +2,983, layered +3,497). The Btree pooling / right-edge append
assumption does not pay off because the `OpenEphemeral` snapshot already spills
the join into a transient and then the rowid append + unique-index insert is not
proving to be append-dominated on these shapes. Racing the packed shape with
`ORDER BY 1` (E4b) is also slower than its unordered E3 base on chain and
layered, confirming sort does not buy appends here.

## E5 best-of combination

The combination that reflects the winners is double-hop plus ordered inserts
(E2's rounds + E4's ordering). It inherits both costs and beats nothing: chain
+8.3 s, layered +17.9 s, grid +1.9 s. Since E1-E4 all lost to baseline on the
large cases, there is no winning combination to assemble; E5's honest result is
that stacking the two least-bad knobs compounds their costs. The only
experiment that ties is E3 on grid, and combining it with round-halving (E5
variant) ran E3's frontier wave with the double-hop join and lost.

## Verdict

No batching or storage trick recovers any of the gap. The dispatch-cost ceiling
is 4 ms (E1), far under the 100 ms bar, and every structural change is slower
or, at best, ties the baseline on one case (E3 grid). The fixpoint cost is the
per-candidate join work plus the btree insert rate, and both are already at the
medium's floor. **Best chain_10000 fixpoint achieved: 8,582 ms, exactly the
unmodified `loop_range_rowid` baseline rerun in the same session.** Chain round
count halved for free (2,580 -> 1,290 via E2) with checksum intact, but that
translates to slower, not faster: the medium is bounded by insert throughput,
not by statement count, and 2,582 dispatch calls cost 4 ms.

## Reproducing

Inputs regenerated deterministically by the shootout harness
(`harness --engines ref --scales 10000`); baseline via
`node run.mjs --input ../dl6/.bench/<case>.in`. Experiments via
`node exp_batch.mjs --input ../dl6/.bench/<case>.in --exp <e1|e2|e3|e4|e5|all>`.
One JSON line per experiment per case.
