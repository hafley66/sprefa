# 2026-07-24 SQLite retraction perf lab

Worktree `lab/sqlite-retract-perf`, based on v11. Harness:
`v6/sprefa-store/target/release/examples/perf_report <engine> <layers> <width> <stride>`,
one hermetic child per engine. Correctness anchor: `out_hash` must equal the oracle's
per cell after every change, and the full `cargo test` suite stays green.

Machine: same as the July matrix in `v6/sprefa-store/PERF-REPORT.md` (reproduced
within 1-2% on 2026-07-24 per the coordinator, so that matrix is a trustworthy
reference).

## Prior art (constrains the hypothesis set; cited, not re-measured)

- **c1c40709** (exp/g3-knob-sweep, `examples/profile_dred.rs` + `examples/explain_plans.rs`):
  the 2s of a DRed retract at 960k is 100% single-core CPU in SQLite's bytecode VM.
  Zero disk I/O (ru_inblock/oublock = 0, db file unchanged, page cache + temp_store=MEMORY
  hold everything), zero network (embedded). Every big-table access is an index seek
  driven from the small wavefront; all 6 nested subqueries plan as rowid-search
  IN-operator seeks. **The one removable cost named: `USE TEMP B-TREE FOR DISTINCT`
  on the 3 hot frontier joins.** This settles seed questions "do the UPDATE-IN-subquery
  statements plan badly" (no) and "do the temp tables spill" (no) without re-measuring.
- **1aa36502** (`FINDINGS-AND-GAPS.md`): "The CTE form is ~20% SLOWER than the loop
  (2.5s vs 2.05s @ 960k), despite 6 statements vs 75. SQLite's recursive CTE
  materializes a working table without the index-driven temp-table frontier the hand
  loop uses. CTE-is-faster was a wrong assumption; measured and rejected." Also:
  counting is wrong on cycles (phantom-cycle rows stay alive); the CYC path can never
  use `sqlite-count`.
- **916e3d5a** (exp/g6-scc-floor): `retract_scc` already collapsed from 3 cone passes
  to 2 (56 -> 39 stmts) with fused per-round mutations and PK `INSERT OR IGNORE`
  replacing `SELECT DISTINCT` temp b-trees. Three further variants were measured and
  reverted (simultaneous frontiers 3248ms; recursive positive-scope CTE 2889ms;
  indexed PK scope 2896ms). The open item it names: no pure-DAG early-out, so scc is
  still ~4.4x plain counting on acyclic cuts.
- **c35685c0** (exp/g2-dred-speed): the findings inventory; G2 names the DRed
  5x-vs-counting gap and suggests (a) skip-rederive detection, (b) fusing over-delete
  and cone-record, (c) precomputing the inner `JOIN cx_cone`.

Consequence: the fresh ground here is (1) the DISTINCT temp b-tree lever applied to
`retract_dred`/`assert` (g6 applied it to `retract_scc` only), (2) the covering-index
question on `ix_cx_dep_child`, (3) ANALYZE / sqlite_stat1, (4) frontier ping-pong to
kill the per-round `DELETE frontier; INSERT frontier SELECT * FROM next` copy, and
(5) page_size. Seed hypotheses 4 and 7 from the dispatch are already answered by
c1c40709 and recorded as prior art above, not re-run.

## Method

Cells: DAG 60k (`6 10000 0`), DAG 960k (`6 160000 0`), CYC 960k s7 (`6 160000 7`).
Engines: `sqlite-count`, `sqlite-count-scc`, `sqlite-dred-loop`, `sqlite-dred-cte`
(oracle once per cell for the reference hash). Every measured number is the
median of >= 3 hermetic runs with the min-max spread reported. A delta inside the
run-to-run spread is noise, not a result. Every change re-verifies `out_hash`
against the oracle and keeps `cargo test` green.

## Baseline (this lab, 3 runs per cell, median [min-max])

Measured 2026-07-24, commit 31045387, runner `tools/lab_run.sh baseline 3`
(raw log /tmp/lab-sqlperf/baseline.log). Reproduces the July matrix within ~2%.

| cell | engine | ms median [min-max] | stmts | correct |
|---|---|---:|---:|---|
| DAG 60k | sqlite-count | 30.8 [30.7-31.5] | 29 | yes |
| DAG 60k | sqlite-count-scc | 119.1 [118.6-120.2] | 39 | yes |
| DAG 60k | sqlite-dred-loop | 132.2 [125.8-182.5] | 75 | yes |
| DAG 60k | sqlite-dred-cte | 160.1 [154.5-162.4] | 6 | yes |
| DAG 960k | sqlite-count | 450.1 [442.9-678.4] | 29 | yes |
| DAG 960k | sqlite-count-scc | 1947.5 [1944.3-2118.9] | 39 | yes |
| DAG 960k | sqlite-dred-loop | 2100.7 [2077.4-2107.2] | 75 | yes |
| DAG 960k | sqlite-dred-cte | 2555.1 [2544.9-2565.1] | 6 | yes |
| CYC 960k | sqlite-count | 394.5 [394.1-401.2] | 29 | NO (phantom cycle, known) |
| CYC 960k | sqlite-count-scc | 2146.7 [2142.9-2167.5] | 39 | yes |
| CYC 960k | sqlite-dred-loop | 2302.9 [2287.6-2308.8] | 75 | yes |
| CYC 960k | sqlite-dred-cte | 2696.4 [2661.4-2716.7] | 6 | yes |

Hash anchors matched: DAG 60k out `21ec8a4cffc3521a`, DAG 960k out
`dd6a707234617ea5`, CYC 960k out `25ee690520b18777` (= oracle each run).

---

## H1: `ix_cx_dep_child` is already covering; an explicit `(child_key, parent_key)` index buys nothing

**Mechanism**: `cx_dep` is `WITHOUT ROWID` with `PRIMARY KEY (parent_key, child_key)`.
SQLite stores a secondary index on a WITHOUT ROWID table as the declared columns
followed by the primary-key columns not already present. `ix_cx_dep_child (child_key)`
therefore physically stores `(child_key, parent_key)`; the reverse walk
(`JOIN cx_dep d ON d.child_key = c.key` then read `d.parent_key`) should already be
answered from the index alone, with no probe back into the base table. If that is
right, the seed-1 concern (index probe + base-table fetch) does not describe what
SQLite does here, and an explicit covering index is pure write amplification.

**Prediction** (before running): `EXPLAIN QUERY PLAN` on the rederive base join shows
`USING COVERING INDEX ix_cx_dep_child`. Adding an explicit `(child_key, parent_key)`
index changes retract time by ~0% (within spread) and only adds bytes and
`add_deps` cost. Verdict expected: the seed hypothesis's premise is wrong, so
the covering variant is REJECTED as a no-op.

**Experiment**: recover `examples/explain_plans.rs` (c1c40709), adapt to the
GraphNs API, confirm the plan line. Then (only if the plan is NOT covering) build
the explicit index variant and measure DAG 960k + CYC 960k on
`sqlite-dred-loop`/`sqlite-count-scc`, plus `add_deps` setup wall time both ways.

**Measured**: `explain_plans` (bundled SQLite 3.51.3, populated 720k-node cyclic
graph) on the rederive base join:

```
PLAN: SCAN c
PLAN: SEARCH d USING COVERING INDEX ix_cx_dep_child (child_key=?)
PLAN: SEARCH p USING INTEGER PRIMARY KEY (rowid=?)
```

and `PRAGMA index_xinfo(ix_cx_dep_child)` on the identical DDL:

```
0|1|child_key|0|BINARY|1
1|0|parent_key|0|BINARY|0
```

The index physically stores `(child_key, parent_key)` because a secondary index
on a WITHOUT ROWID table appends the missing primary-key columns.

**Verdict**: REJECTED (the seed premise). The reverse walk is already index-only;
`ix_cx_dep_child` IS the covering `(child_key, parent_key)` index under another
name, and SQLite already plans it as COVERING. An explicit second index would
duplicate those bytes and slow `add_deps` for zero read gain. No timing run
needed; the plan line plus the physical layout decide it.

## H2: replacing `INSERT INTO ... SELECT DISTINCT` with `INSERT OR IGNORE` in `retract_dred` removes the temp-b-tree dedup and speeds the loop 5-15%

**Mechanism**: c1c40709 names `USE TEMP B-TREE FOR DISTINCT` as the only removable
cost in the DRed plans. `SELECT DISTINCT` dedups through an ephemeral b-tree, then
the INSERT dedups AGAIN through the target temp table's primary key (cx_next /
cx_frontier are `key INTEGER PRIMARY KEY`). Two b-trees doing one job. 916e3d5a
took this lever for `retract_scc` (part of 56 -> 39); `retract_dred` (engine.rs:465,
:489, :502) and `assert` (:416) still carry the DISTINCT. scc's total win over
dred-loop was ~6% and included statement fusion, so the DISTINCT share alone is
likely a one-digit percent.

**Prediction**: `sqlite-dred-loop` DAG 960k drops from ~2.05s to ~1.85-1.95s
(5-10%); CYC 960k similar ratio; the crossed-zero logic is untouched so hashes are
identical. `sqlite-count` unchanged (it uses GROUP BY, not DISTINCT).

**Experiment**: edit the three DRed statements (+ assert's one) to
`INSERT OR IGNORE ... SELECT` (no DISTINCT); rebuild; 3x each cell on
`sqlite-dred-loop`; EQP before/after showing the temp-b-tree line gone;
full `cargo test`.

**Measured** (3 runs, median [min-max], vs B0):

| cell | engine | B0 med | H2 med [min-max] | delta |
|---|---|---:|---:|---:|
| DAG 60k | sqlite-dred-loop | 132.2 | 122.2 [120.1-137.4] | -7.6% |
| DAG 960k | sqlite-dred-loop | 2100.7 | 1970.3 [1958.0-1971.1] | -6.2% |
| CYC 960k | sqlite-dred-loop | 2302.9 | 2142.8 [2132.1-2148.2] | -7.0% |
| DAG 960k | sqlite-count (untouched) | 450.1 | 440.6 [437.3-453.4] | -2.1% (noise) |
| DAG 960k | sqlite-count-scc (untouched) | 1947.5 | 1968.2 [1951.1-1972.9] | +1.1% (noise) |
| DAG 960k | sqlite-dred-cte (untouched) | 2555.1 | 2516.9 [2506.0-2538.0] | -1.5% (noise) |

Out-hashes equal the oracle on every run; `cargo test --release` fully green
(23+3+1+1+3+6+7+1+4+4+1+1+1 passed, 1 pre-existing ignored). EQP after: zero
`USE TEMP B-TREE FOR DISTINCT` lines remain in the cascade plan set (the only
temp b-tree left is counting's GROUP BY aggregation, which does real work).

**Verdict**: CONFIRMED. The deciding receipt is the pair: the plan line vanished
AND dred-loop moved -6.2/-7.0% at 960k while every untouched engine stayed
inside its spread. dred-loop now ties retract_scc (1970 vs 1968 DAG 960k;
2143 vs 2133 CYC 960k), which is consistent with 916e3d5a's claim that scc's
~6% edge over dred came from exactly this lever.

## H3: the scc/dred 4.4x gap vs counting on DAG cuts is cone amplification (work proportional to the whole reachable cone, twice), not a few expensive rounds

**Mechanism**: on this benchgraph, every derived node is forward-reachable from
root 0 (layer-1 nodes all have parent 0), so the over-delete cone is ~960k nodes
even though only ~160k actually die; the rederive then walks ~800k back. Counting
touches only the ~160k truly-dead wavefront. Work is edge probes: dred/scc do
~2x total-edges probes, counting does ~dead-set-edges probes. If right, per-round
timing shows cost spread across all rounds of both phases roughly proportional to
wavefront width, and the two phases each take roughly half; no single expensive
round. This would mean the remaining gap is algorithmic (g6's missing pure-DAG
early-out), not a SQL-shape defect, and constant-factor SQL work cannot close it.

**Prediction**: DL_CASCADE_TRACE per-statement timing over a DAG 960k
`retract_scc` shows phase 1 (cone kill) and phase 2 (rederive) each ~40-60% of
total, cost per round tracking wavefront size, no outlier round.

**Experiment**: `DL_CASCADE_TRACE=1` run of `sqlite-count-scc` and
`sqlite-dred-loop` at DAG 960k; aggregate stderr per phase.

**Measured** (post-H2 code, DAG 960k; traces in /tmp/lab-sqlperf/h3-*.trace):

- `sqlite-dred-loop`, 1990 ms traced: over-delete phase ~1030 ms (7 expansion
  joins 19-85 ms each, kill UPDATEs 40-45 ms, cone inserts 20-44 ms, frontier
  copies ~18-20 ms each), rederive phase ~960 ms (base reverse join 248 ms, then
  rounds of 52-95 ms expansions + 33-41 ms UPDATEs). Largest single statement is
  the rederive base at 248 ms = 12% of total; everything else is a per-round cost
  tracking wavefront width.
- `sqlite-count-scc`, 1978 ms traced: same shape (phase 1 ~1021 ms, phase 2
  base 273 ms fused, phase 2 rounds ~684 ms).
- Side observation that seeded H5: the per-round `frontier <- next` copies sum
  to ~210 ms (~10%) in dred-loop.

**Verdict**: CONFIRMED. The cost is spread evenly across all rounds of both
phases in proportion to wavefront size; there is no expensive-round outlier. The
4.4x-vs-counting gap is cone amplification (the over-delete cone is ~960k nodes
where only ~160k die, then ~800k are rederived), which is algorithmic (g6's
missing pure-DAG early-out), and constant-factor SQL levers can only shave the
per-probe cost, not the probe count.

## H4: ANALYZE / sqlite_stat1 does not move retract time, because CROSS JOIN already pins every join order

**Mechanism**: the cascade's hot joins use `CROSS JOIN`, which in SQLite disables
join reordering, and the IN-subquery UPDATEs already plan as rowid seeks
(c1c40709). Stats can only change plans where the planner has a choice; the only
remaining choices are index selection on cx_dep (dictated by the ON clause
columns) and the recursive CTE joins in `retract_dred_cte` (plain JOIN, so the
planner CAN reorder those). So: no effect on count/scc/dred-loop; a possible
effect (either sign) on dred-cte only.

**Prediction**: count/scc/dred-loop within spread of baseline; dred-cte moves
< 10% either way (weak prior; stats on a 2-integer-column b-tree rarely flip a
2-table recursive step).

**Experiment**: run `ANALYZE` after load (untimed setup, before the measured
retract) via a harness env hook (`DL_LAB_ANALYZE=1`); 3x all cells on all four
engines, compared against the same code without the hook.

**Measured** (vs the post-H5 reference, medians):

| cell | engine | no stats | ANALYZE | delta |
|---|---|---:|---:|---:|
| DAG 960k | sqlite-count | 427.5 | 433.1 | +1.3% (noise) |
| DAG 960k | sqlite-count-scc | 1788.0 | 1798.5 | +0.6% (noise) |
| DAG 960k | sqlite-dred-loop | 1788.3 | 1805.6 | +1.0% (noise) |
| DAG 960k | sqlite-dred-cte | 2548.1 | 2747.6 [2741.6-2759.7] | **+7.8%** |
| CYC 960k | all four | | | -2.1% to +1.8% (noise) |
| DAG 60k | all four | | | +2.0% to +4.0% |

EQP diff (explain_plans with/without ANALYZE): the only plan that moves is the
CTE phase-2 base case, where stats reorder the probe sequence after the
`SCAN d USING COVERING INDEX ix_cx_dep_child` from (p, c) to (c, p). Every
CROSS JOIN statement is plan-identical with and without stats, as predicted.

**Verdict**: CONFIRMED for the loop engines (CROSS JOIN pins the plan; stats
change nothing beyond noise) and the "either sign" hedge for dred-cte landed on
the bad side: stats make the CTE measurably SLOWER at DAG 960k. Actionable
consequence: do NOT add ANALYZE to the store; the current no-stats behavior is
the right default. Bonus finding: the no-stats CTE phase-2 base plans as a FULL
SCAN of `ix_cx_dep_child` driving into p/c probes, instead of scanning the cone
and probing the child index the way the loop's base does. That is a join-order
defect in the CTE SQL itself (plain JOIN leaves the planner free) and it becomes
H7.

## H5: frontier ping-pong (role swap in Rust instead of `DELETE frontier; INSERT frontier SELECT FROM next`) removes a full extra copy of every wavefront row and speeds dred-loop/scc ~5-15%

**Mechanism**: each round ends with `DELETE FROM frontier` + `INSERT INTO frontier
SELECT key FROM next`, a b-tree insert per wavefront row into a second temp table,
plus the delete churn. Over a whole DRed run that is ~cone + rederive rows
(~1.76M extra temp b-tree inserts at 960k) doing zero algorithmic work. Since the
SQL is `format!`ed per round anyway, the Rust loop can swap which physical table
plays "frontier" and which plays "next" each round; the copy disappears. Same for
`retract` (counting) and `assert`, with smaller absolute savings (smaller
wavefronts).

**Prediction**: dred-loop DAG 960k gains 5-15% (temp-RAM b-tree inserts are
cheap but 1.7M of them is not free); counting gains less in absolute ms.
Compounds with H2. Hashes identical (pure copy elimination).

**Experiment**: implement swap in `retract_dred` first (isolated), measure 3x
all three cells, `cargo test`; then, if confirmed, extend to `retract_scc`
(careful: its fused multi-statement strings bake both names into one exec) and
`retract`.

(Executed as one wave across `retract`/`assert`/`retract_dred`/`retract_scc`;
the H3 trace had already shown the copies are a clean ~10% of dred-loop.)

**Measured** (3 runs, median [min-max], vs post-H2):

| cell | engine | H2 med | H5 med [min-max] | delta | stmts |
|---|---|---:|---:|---:|---|
| DAG 60k | sqlite-count | 31.3 | 29.7 [29.6-29.9] | -5.3% | 29 -> 23 |
| DAG 60k | sqlite-count-scc | 120.6 | 110.8 [109.1-111.2] | -8.1% | 39 (fused) |
| DAG 60k | sqlite-dred-loop | 122.2 | 109.9 [108.9-110.0] | -10.1% | 75 -> 53 |
| DAG 960k | sqlite-count | 440.6 | 427.5 [426.3-429.7] | -3.0% | 29 -> 23 |
| DAG 960k | sqlite-count-scc | 1968.2 | 1788.0 [1782.0-1788.3] | -9.2% | 39 |
| DAG 960k | sqlite-dred-loop | 1970.3 | 1788.3 [1782.7-1795.8] | -9.2% | 75 -> 53 |
| CYC 960k | sqlite-count-scc | 2132.7 | 1951.0 [1950.1-1955.7] | -8.5% | 39 |
| CYC 960k | sqlite-dred-loop | 2142.8 | 1966.0 [1951.0-1987.7] | -8.3% | 75 -> 53 |
| DAG/CYC | sqlite-dred-cte (untouched) | | +0.3% to +1.5% | noise | 6 |

All hashes = oracle; full `cargo test --release` green (including the two-pass
cycle tests and stmt_count).

**Verdict**: CONFIRMED, at the top of the predicted 5-15% band for the two-pass
engines. The receipt is the pairing of the removed statements (75 -> 53, and the
scc fused string losing its DELETE+INSERT tail) with a drop that matches the
~210 ms the H3 trace attributed to exactly those statements (dred-loop DAG 960k
-182 ms vs ~207 ms traced copy cost; the remainder is the copies' cache
pressure being partly overlapped).

## H6: page_size does not materially move an all-in-RAM retract

**Mechanism**: the working set is fully in page cache (c1c40709: zero disk I/O),
so page_size only changes b-tree fanout and page-search width, both CPU-side.
Bigger pages = shallower trees but wider in-page binary searches; these mostly
cancel for integer-key tables. Must be set before table creation to take effect.

**Prediction**: 8192 vs default 4096 at DAG 960k: within spread or < 5% either
way on all engines. Recorded mostly to close seed 6 with a receipt.

**Experiment**: harness env hook `DL_LAB_PAGE_SIZE` sets the pragma before the
first table create; 3x all cells at 8192, plus a focused 16384 probe at DAG
960k.

**Measured** (vs post-H5 reference, medians):

| cell | engine | 4096 (default) | 8192 | 16384 |
|---|---|---:|---:|---:|
| DAG 60k | sqlite-count | 29.7 | 25.2 [25.1-25.8] (-14.9%) | |
| DAG 60k | sqlite-count-scc | 110.8 | 100.9 (-9.0%) | |
| DAG 60k | sqlite-dred-loop | 109.9 | 101.4 (-7.7%) | |
| DAG 960k | sqlite-count | 427.5 | 422.8 (-1.1%) | 423.8 (-0.9%) |
| DAG 960k | sqlite-dred-loop | 1788.3 | 1765.8 (-1.3%) | 1769.9 (-1.0%) |
| DAG 960k | sqlite-count-scc | 1788.0 | 1766.2 (-1.2%) | |
| CYC 960k | all four | | -0.6% to -1.6% | |

Hashes = oracle throughout.

**Verdict**: CONFIRMED at the scale that matters (960k: everything within ~1.5%,
8k and 16k indistinguishable), but the prediction under-called the small-db
case: DAG 60k speeds up 8-15% at 8192, consistent with the 3.76 MB db losing a
b-tree level (fewer page descents per probe) while the 61 MB db keeps the same
depth either way. Not adopted: the win exists only where absolute times are
already ~30-110 ms, and page_size is frozen at db creation, so tuning it for
small corpora would pessimize nothing but also buy nothing at target scale.

## H7: pinning the CTE phase-2 base join order (CROSS JOIN, cone-driven) removes its full index scan and speeds dred-cte 5-15%

**Mechanism**: EQP (see H4) shows the no-stats plan for the CTE rederive base is
`SCAN d USING COVERING INDEX ix_cx_dep_child` -> probe p -> probe c: it walks
ALL ~1.9M dep index entries and probes cx_row for each, because plain `JOIN`
leaves the planner free and the temp cone table has no stats. The loop version
of the same logic (`rd base` plan) drives `SCAN c` -> `SEARCH d (child_key=?)`
-> `SEARCH p`, touching only the cone's incoming edges. Rewriting the CTE base
case with `CROSS JOIN` in loop order should replace the 1.9M-entry scan with a
cone-driven probe walk. The recursive step already plans cone-driven and keeps
its shape.

**Prediction** (before running): dred-cte DAG 960k drops 5-15% (the base's
excess is bounded by one full index scan + ~1.9M row probes, worth a few
hundred ms against a 2548 ms total; the loop's equivalent base costs 248 ms).
No effect on the other engines. Hashes unchanged (UNION semantics untouched).

**Experiment**: change `retract_dred_cte` phase-2 base (and phase-1 walk for
consistency where safe) to CROSS JOIN pinned order; EQP re-check; 3x all
cells on `sqlite-dred-cte`; `cargo test`.

**Measured**: EQP after the pin shows the wanted plan (`SCAN c` ->
`SEARCH d USING COVERING INDEX ix_cx_dep_child (child_key=?)` -> `SEARCH p`).
Timing (3 runs, median, vs the unpinned post-H5 reference):

| cell | unpinned | pinned | delta |
|---|---:|---:|---:|
| DAG 60k | 154.0 | 164.4 [164.3-165.3] | +6.8% |
| DAG 960k | 2548.1 | 2748.7 [2736.5-2773.2] | +7.9% |
| CYC 960k | 2739.7 | 2898.9 [2889.9-2899.6] | +5.8% |

Hashes = oracle on every run (the pin is semantics-neutral, as expected).

**Verdict**: REJECTED, and the code is reverted; the receipt is the tight-spread
+6-8% regression on every cell. The mechanism reading: after the phase-1 kill,
the cone holds ~960k of 960k reachable nodes, so "cone-driven" probing costs
~1M separate index descents (one per cone member) plus the same ~1.9M entry
reads, while the planner's full covering-index scan is ONE sequential leaf walk
whose `p.weight>0` filter (parents outside the cone, essentially root 1) is far
more selective than cone membership. At this cone/graph ratio the full scan is
the better plan, and the no-stats planner already picks it. This also completes
the H4 story: ANALYZE's +7.8% came from re-ordering the same base's probes to
check the unselective cone membership before the selective parent-weight test.
The loop-shaped base (248 ms in the H3 trace) is not a plan the CTE should
imitate; the CTE's base was never its problem. What remains against the loop is
the recursive step's row-at-a-time queue processing, which is structural to
SQLite recursive CTEs (1aa36502's settled result, now with a sharper boundary:
only the recursion, not the base, is the deficit).

---

## B0: baseline reproduction (no code change)

(tables above in "Baseline"; raw logs /tmp/lab-sqlperf/*.log)
