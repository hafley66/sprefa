---
created: 2026-08-23
updated: 2026-08-23
type: improvement
reporter: hafley66
status: open
priority: high
related: ['@engine-tick-trace']
labels: [engine, performance]
---

# Ordered tick recomputes every level and snapshots every rel: O(rels) SQL per tick, not O(change)

_Source: v6/sprefa-engine-rs/src/ordered.rs_

## Description

## Description

A program with `<+` rules runs `ordered.rs::run_tick` (`program.rs:180`), and that path does O(rels) SQL work per tick whether or not anything changed. Measured 2026-08-23 on `ghcache.dl6` (154 rels, 100 level rels, 52 ordered arms), tick 5 of the scripted schedule, ONE arrival:

| phase | statements | returned and changed nothing | source |
|---|---:|---:|---|
| `read_snapshot` x5 per tick (before decoded, before stored, mid, after decoded, after stored) | 617 | 424 | `ordered.rs:714,715,741,753,754`, each loops all 154 `final_select` |
| `recompute_levels` x2 per tick, every level: `DELETE FROM rel; INSERT ... SELECT <full join>` | 311 | 206 | `ordered.rs:739,749` -> `:260-263` `recompute_sql` |
| frontier clear `DELETE __f_*`, `__next_f_*` per rel | 154 | 154 | `incremental.rs:937` via `stage_ordered_frontiers` |
| occurrence arms | 35 | 32 | `ordered.rs:746` |
| `execute_multiple` batches | 202 | | not in the count above |

1,135 statements + 202 batches for one arrival; 821 inert. ~20 us each, 23 ms of SQLite per tick; the cost is count, not weight. An idle tick (0 arrivals) costs the same.

Deltas are produced by diffing the before and after snapshots in Rust (`build_deltas`, `ordered.rs:755`), which is why every rel is read five times: the path has no per-rel change signal, so it reads everything to find out.

The frontier machinery already exists and is unused here: `incremental.rs` `__f_<rel>` tables, `promote_frontiers`, and a "which levels read which frontier" reachability map at `incremental.rs:1004-1040` (`reads_frontier_of`). The seam returns `rows_changed` on every write (`sql.rs` `QueryResult`), so a per-rel dirty bit costs no extra SQL.

ARCH.pl:855 pinned this on 2026-07-30 ("ordered/pre 13+2n statements per tick, zero-arrival full-table SCAN") and assigned the fix to `pre_occurrence_loop`, which landed without removing the recompute.

## How DD and Feldera do it

| engine | idle operator cost | mechanism |
|---|---|---|
| differential dataflow | 0 | timely's scheduler activates an operator only when a message lands on its input |
| Feldera / DBSP | nanoseconds | the circuit steps every operator each clock, but `eval` on an empty Z-set batch returns empty without touching the trace |
| sprefa ordered path | ~160 us per rel | 8 SQL round trips per rel per tick regardless of input |

Both are O(change) because the idle path is in-memory and the operator reads its delta, not its base tables.

## Deliverable

1. Dirty set. `run_tick` keeps `dirty: HashSet<rel>` for the tick: arrival targets from `apply_arrivals`, plus every rel whose write returned `rows_changed > 0`. No SQL to compute it.
2. `recompute_levels` recomputes a level only when some rel it reads is dirty (reuse `reads_frontier_of` from `incremental.rs:1004`; move it to a shared fn, do not copy). A recomputed level whose row set did not change (compare `rows_changed`, or count before/after) does not mark itself dirty.
3. `read_snapshot` reads only dirty rels for the before/after diff; the `before` snapshot for a rel is read lazily the first time the rel becomes dirty in the tick (before its first write), so the semantics of `build_deltas` are unchanged.
4. Frontier clears run only for rels whose frontier was non-empty last tick (the seam's `rows_changed` on the staging INSERT is the bit).
5. COUNT test, additive, in `sprefa-engine-rs/tests/`: fold `ghcache.schedule.json`, assert statements per zero-arrival tick <= `2 + (rels touched by the clock)` and per one-arrival tick bounded by the dependency cone of the arrival rel, using the per-verb counts from @engine-tick-trace. Also assert the tick log is byte-identical to the pre-change log (grade.sh byte-clean count does not drop).
6. Gates: conformance, plunit, grade.sh byte-clean unchanged, cargo, ghcache gate `ticks=11`, goldens 6. Before/after statement count per tick in the PR body, three runs each.
7. `docs/failure-modes.md` entry; flip ARCH.pl:855's F5 text.

## Not in scope

Replacing the ordered path with the incremental path wholesale, or any DBSP/Z-set kernel ("i only want emitters", CLAUDE.md). The language surface is untouched.

Depends on @engine-tick-trace for the per-verb counts the COUNT test reads.
