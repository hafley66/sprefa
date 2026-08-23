---
created: 2026-08-23
updated: 2026-08-23
type: improvement
reporter: hafley66
status: open
priority: low
related: ['@ordered-tick-recompute']
labels: [engine, performance]
---

# stage_ordered_frontiers clears a next-frontier table the ordered path never writes

_Source: v6/sprefa-engine-rs/src/incremental.rs:902-908_

## Description

incremental.rs:902-908 stage_ordered_frontiers issues a DELETE on the __next_f_<rel> table for every rel each tick; the ordered path never writes those tables, so every one of those statements is inert. Worth about one statement per rel per tick (154 on ghcache). Fix: skip the next-frontier clear when the program is ordered, or clear only rels whose staging insert reported rows_changed > 0. Receipt: per-tick statement count drop in tests/ordered_statement_count.rs, tick log byte-identical.
