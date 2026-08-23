---
created: 2026-08-23
updated: 2026-08-23
type: bug
reporter: hafley66
status: open
priority: high
related: ['@one-tick-path', '@incremental-empty-delta-skip']
labels: [engine, performance]
---

# #427 doubles ghcache fold cost: 7,113 -> 16,655 statements, 150 -> 286 ms; recount 8,279 calls and level_insert 45 us each

_Source: v6/sprefa-engine-rs/src/incremental.rs_

## Description

Measured 2026-08-23 by the coordinator, release emit_rust_harness, ghcache.schedule.json (14 ticks), DL_TRACE_SUMMARY, three runs each, identical counts: pre-#427 (d12fc053f, ordered.rs + #423 dirty set) statements=7113 sqlite 130-134 ms wall 147-152 ms; post-#427 (ecce409d5) statements=16655 sqlite 265-270 ms wall 284-290 ms. Per verb (us / calls), new: level_insert 85139/1873, recount 48226/8279, publish 9516/324, clear 8469/41, stage 8406/569, probe 4851/14, aggregate 3226/482. Old: recompute 23879/2367, snapshot 7918/910, stage_carry 2405/413. DDL ~80 ms both. Reading: on ghcache's tiny tables (tens of rows) the incremental delta insert costs 45 us a call against 10 us for a rebuild-from-base, and the refcount maintenance (recount, 8,279 calls) has no skip at all. The idle tick (3 statements) and the deleted ordered.rs are right; the busy tick is not. Candidates, each with a COUNT receipt: (1) recount only for heads whose level_insert reported rows_changed > 0; (2) incremental-empty-delta-skip on the per-rel fixed verbs (publish/clear/stage); (3) per-level choice of rebuild-vs-delta by base table size (a rebuild is cheaper under N rows; measure N); (4) batch recount into one statement per head. Receipt for closing: ghcache fold statements and wall at or below the pre-#427 numbers above, tick log byte-identical, grade.sh 340.

## Comments

### 2026-08-23T18:05:16Z · @sprefa-coordinator

PR #430 (f7bce8702): ghcache fold 276 -> 235 ms, statements 14955 -> 11534; recount 8279 -> 5630, level_insert 1873 -> 1284. Receipt (7113 / 152 ms) NOT reached; the remainder is lower.pl's 2^N delta arms (issue delta-arm-subset-expansion) and recount's from-scratch re-derive (issue recount-waits-for-a-retraction). Stays open until those land.

### 2026-08-23T21:11:35Z · @sprefa-coordinator

#434+#435+#436+#433: fold 11,534 -> 6,738 statements, wall 235 -> 191 ms, page_response 7,284 -> 152 us/call. Statements beat the 7,113 target; wall still above the 152 ms pre-#427 receipt. Left open for the wall remainder.

