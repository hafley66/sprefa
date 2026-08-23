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
