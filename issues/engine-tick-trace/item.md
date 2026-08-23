---
created: 2026-08-23
updated: 2026-08-23
type: bug
reporter: hafley66
status: open
priority: high
labels: [engine, self-diagnosis]
---

# Engine tick trace: live sqlite_ms reads 0 and every ordered statement is unlabelled

_Source: v6/sprefa-engine-rs/src/{run,ordered,trace,executors/cost}.rs_

## Description

## Description

The engine cannot say where a tick's time goes. Three holes, measured 2026-08-23 on `v6/dl/ghcache/ghcache.dl6`, release `emit_rust_harness`, scripted `ghcache.schedule.json`:

| hole | evidence | where |
|---|---|---|
| live `engine_tick_cost` rows read `wall_ms=0 sqlite_ms=0` on every bucket | `~/.agent/dl6.db` `__txt_ghcache_engine_tick_cost`, 9 rows, all zero except `rss_kb` | `executors/cost.rs:86` reads the trace table; `trace.rs:81` records only under `DL_TRACE_SUMMARY` or `trace::force_summary()`; `run.rs` calls neither (`arm()` at `run.rs:694` pins the clock, it does not enable recording) |
| every statement of the ordered path is `verb="unlabelled" relation=-` | `DL_TRACE_SUMMARY=1` table: `603521 us unlabelled, 15603 calls`; tick-5 trace: 1135/1135 statements unlabelled | `ordered.rs` has 0 `Scope::verb` sites (`incremental.rs` has 18); `trace::current_label()` falls back to `unlabelled` when no frame is open |
| `DL_SEAM_SHAPES` shows inert statements only and caps at 20 | `sql.rs:345-365` `report_seam_tally`, `INERT_SHAPES` | no per-rel, per-verb statement count reaches a rel |

Self-diagnosis law (CLAUDE.md): the system answers "what was it doing" from its own on-disk trail. Today the trail says 0.

## Deliverable

1. `run.rs` resident runner calls `crate::trace::force_summary()` before the first fold (beside `arm()` at `run.rs:694`), so `engine_tick_cost.sqlite_ms` and `wall_ms` are real numbers in `~/.agent/dl6.db`. Receipt: a live bucket row with `sqlite_ms > 0`.
2. `ordered.rs` opens a `Scope::verb(<verb>, <rel>, <strategy>)` around every per-rel statement group: `read_snapshot` per rel (`ordered.rs:33`), `recompute_levels` per level (`ordered.rs:224-264`), `apply_occurrence` per arm, `apply_retention`, `stage_ordered_frontiers`, `stage_departures`. Verb names come from the existing six (`stage`, `arrive`, `clear`, `read_staged`, `edge_lookup`, `edge_write`) plus `snapshot` and `recompute`; add the two to the `trace.rs` doc comment.
3. `tick_cost` host rows carry one row per `(verb, rel)` with non-zero calls, not just the `wall` row (`cost.rs:105-120` already emits per-label rows once labels exist).
4. COUNT test in `sprefa-engine-rs/tests/`: fold `ghcache.schedule.json` with the summary forced, assert `unlabelled` calls == 0 and the `recompute` verb has exactly `levels.len() * 2` calls per tick (pins the defect in @ordered-tick-recompute until that lands; update the number when it does).
5. `docs/failure-modes.md` entry: incident, RCA, fail-pre-fix test, rail.

## Reading the numbers today

```
cd v6 && DL_ADAPTERS_DIR=$PWD/dl/ghcache DL_TRACE_SUMMARY=1 RUST_LOG=sprefa_engine_rs=info \
  sprefa-engine-rs/target/release/emit_rust_harness <compiled>.rs dl/ghcache/ghcache.schedule.json --final
```
prints the `== DL_TRACE_SUMMARY ==` table on stderr. Release, 11 ticks: wall 366 ms, sqlite 343 ms (93.7%), DDL 84 ms once, rule SQL 248 ms, Rust 23 ms.

## Out of scope

Reducing the statement count. That is @ordered-tick-recompute.

## Comments

### 2026-08-23T06:18:45Z · @sprefa-coordinator

Items 1, 4 (wall row only), 5 landed in PR #424 (cb72bce75). Items 2 (Scope::verb in ordered.rs) landed in PR #423 (30fbd3669). Item 3 (the _recent GraphQL selection, pr_transition open->merged) is PR 2, in flight on lane fix-engine-tick-trace.
