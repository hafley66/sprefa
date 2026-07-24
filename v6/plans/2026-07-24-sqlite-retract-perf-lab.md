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

**Measured**: (pending)

**Verdict**: (pending)

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
retract) via a temporary harness hook; 3x DAG 960k on all four engines; then
revert the hook.

**Measured**: (pending)

**Verdict**: (pending)

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

**Measured**: (pending)

**Verdict**: (pending)

## H6: page_size does not materially move an all-in-RAM retract

**Mechanism**: the working set is fully in page cache (c1c40709: zero disk I/O),
so page_size only changes b-tree fanout and page-search width, both CPU-side.
Bigger pages = shallower trees but wider in-page binary searches; these mostly
cancel for integer-key tables. Must be set before table creation to take effect.

**Prediction**: 8192 vs default 4096 at DAG 960k: within spread or < 5% either
way on all engines. Recorded mostly to close seed 6 with a receipt.

**Experiment**: add `PRAGMA page_size=8192` at the top of `create_schema`
(before DDL, after connection open), rebuild, 3x DAG 960k on `sqlite-count` and
`sqlite-dred-loop`, revert.

**Measured**: (pending)

**Verdict**: (pending)

---

## B0: baseline reproduction (no code change)

(tables below are filled as runs complete)
