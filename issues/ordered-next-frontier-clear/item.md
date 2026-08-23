---
created: 2026-08-23
updated: 2026-08-23
type: improvement
reporter: hafley66
status: done
priority: low
related: ['@ordered-tick-recompute']
labels: [engine, performance]
closed: 2026-08-23
---

# stage_ordered_frontiers clears a next-frontier table the ordered path never writes

_Source: v6/sprefa-engine-rs/src/incremental.rs:902-908_

## Description

incremental.rs:902-908 stage_ordered_frontiers issues a DELETE on the __next_f_<rel> table for every rel each tick; the ordered path never writes those tables, so every one of those statements is inert. Worth about one statement per rel per tick (154 on ghcache). Fix: skip the next-frontier clear when the program is ordered, or clear only rels whose staging insert reported rows_changed > 0. Receipt: per-tick statement count drop in tests/ordered_statement_count.rs, tick log byte-identical.

## Resolution

### 2026-08-23T17:50:55Z · @issuectl

stage_ordered_frontiers had no Rust caller after #427 deleted ordered.rs, so the per-rel next-frontier DELETE this issue names was unreachable code; the function is deleted in the one-path-busy-tick-cost PR. The live equivalents now carry the same saving: incremental.rs records which frontier and carry tables a tick actually wrote (TickWork::carries / holds_frontier), and the mid-tick merge and end-of-tick promote run only for those rels.
