---
created: 2026-08-23
updated: 2026-08-23
type: improvement
reporter: hafley66
status: open
priority: normal
related: ['@ordered-tick-recompute']
labels: [engine, performance]
---

# Derive the decoded delta from the stored delta instead of re-reading the decoded table

_Source: v6/sprefa-engine-rs/src/ordered.rs_

## Description

After #423 a one-arrival ghcache tick is ~367 statements (cap 450); the widest arrival moves 25 rels and runs 37 level recomputes over two passes. The next cut the lane named: read_snapshot reads each moved rel twice (decoded and stored) to build both delta views; the decoded delta is a pure function of the stored delta plus the rel's column types (the same decode final_select applies), so one read per moved rel suffices. Receipt: tests/ordered_statement_count.rs caps lowered to the new measurement, tick log byte-identical, grade.sh byte-clean 340 unchanged.

## Comments

### 2026-08-23T17:51:11Z · @one-path-busy-tick-cost

Not touched by the one-path-busy-tick-cost PR and still valid. This issue is about read_snapshot reading each moved rel twice on ordered.rs, which #427 deleted; the equivalent double read on the one path is enum_plane::decode_deltas over read_boundary's output, which is a different seam. Needs re-measuring against the current path before it is worth pricing: on the ghcache fold after this PR, decode is 0 us and publish is 9.5 ms over 316 calls, so the double read this issue names is no longer where the money is.

### 2026-08-24T01:10:57Z · @sprefa-coordinator

User direction 2026-08-23: juice from the storage side; program IN SQLite (triggers probe live on probe-trigger-delta), keep hot work inside the db engine rather than round-tripping statements; a postgres emitter (@postgres-emitter) is a legitimate future door off the same lowered plan.

### 2026-08-24T01:13:53Z · @sprefa-coordinator

Trigger probe result (branch probe/trigger-delta, last copy cdab0e954, branch deleted after this note): AFTER-write triggers populating __delta cut ghcache fold 6738 -> 5845 statements (-13.3%) but RAISED tick SQLite time 88.2 -> 91.5 ms (+3.8%); removed statements were the cheap ones (stage ~13 us each), write verbs absorbed the trigger row-work (+~4.9 ms across level_insert/recount/aggregate/edge_write). Receipts green both arms; frontier staging not movable (_phase/_sequence are runtime state). Verdict: NO LANDING. Pricing insight: the ~25 us/statement floor is wrong; cheap statements run ~13 us and per-row work dominates write verbs, which lowers the projected win of in-memory deltas accordingly.

### 2026-08-24T03:44:17Z · @sprefa-coordinator

Both probe arms measured, both NO LANDING (branches deleted; last copies: inmem-carry 89e3074ee, returning 9d6c3a0). RETURNING arm: statements -4.7% but wall +32%; the write re-evaluates the projection per ROW (json CASE + dictionary subqueries) where publish did it per GROUP. INMEM-CARRY arm: correct and cheap but nearly useless as-is; staged rows carry interned INTEGER ids while readers want decoded text, so only provably-empty reads (91, carrying 8 rows) were served from memory. CEILING measurement (undecoded rerun): decode + __txt view = 77% of publish time on ghcache, 65% on wide_64; the true removable per-statement overhead is 4.9-8.9 us, not 25. The design that pays is the issue title literally: move the decode expression and text un-intern into Rust (needs an id->text plane and a v6/prolog contract change). That is language-adjacent; Chris decides before any lane.



