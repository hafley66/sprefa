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
