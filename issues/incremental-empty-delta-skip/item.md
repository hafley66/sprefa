---
created: 2026-08-23
updated: 2026-08-23
type: improvement
reporter: hafley66
status: open
priority: high
related: ['@one-tick-path', '@ordered-tick-recompute']
labels: [engine, performance]
---

# Incremental path runs every level's SQL every tick: gate each operator on a non-empty input frontier

_Source: v6/sprefa-engine-rs/src/incremental.rs:1319-1372_

## Description

The incremental path (programs on incremental.rs, and every program once one-tick-path lands) runs each level's select_sql every tick regardless of whether any frontier it reads holds a row; DBSP's equivalent is an operator returning in nanoseconds on an empty batch, ours is a SQLite prepare+step (~20 us). Measured (#419 report): wide_64, 128 rels, 5,767 statements over 3 ticks = ~15 per rel per tick, deterministic. Fix, same mechanism as #423: a per-tick dirty set fed by rows_changed on the staging inserts; a level runs only when a rel in reads_by_head(level) is dirty; frontier clear/promote/read_staged only for rels whose frontier was non-empty. No new tables. Receipts: COUNT test over the shared_frontier_wide programs with caps per tick (idle tick under 2 + clock), tick logs byte-identical, grade.sh byte-clean unchanged. Sequenced after one-tick-path (same file).
