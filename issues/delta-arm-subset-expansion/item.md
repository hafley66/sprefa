---
created: 2026-08-23
updated: 2026-08-23
type: bug
status: open
priority: high
related: ['@one-path-busy-tick-cost']
labels: [engine, performance]
---

# A level's delta insert is one arm per SUBSET of its body: 2^8 = 256 arms on ghcache page_response, 5.6x slower than the same rule's rebuild

## Description

_Source: v6/prolog/lower.pl (delta plan), read through the emitted `levels[i].insert_sql`_

## Description

Measured 2026-08-23 on `fix/one-path-busy-tick-cost` after the level clock gate landed, release `emit_rust_harness`, ghcache.schedule.json, tables settled after the 14-tick fold, three timings per statement, steady state after the first (prepare-dominated) call.

`levels[i].insert_sql` for a rule with N body items carries 2^N `UNION ALL` arms, not the N arms incremental view maintenance needs (one per body item, driven by that item's frontier against the others' full base tables). Per level on ghcache:

| head | arms | insert_sql | rebuild stmts | delta us | rebuild us |
|---|---|---|---|---|---|
| page_response | 256 | 248 KB | 64 | 3661, 3832, 4161 | 670, 677, 8891 (first) |
| poll_state | 48 | 27 KB | 16 | 306, 306, 340 | 126, 127, 820 (first) |
| pull_request_seen | 16 | 18 KB | 8 | 146, 148, 161 | 138, 141, 1405 (first) |
| period_candidate | 15 | - | 5 | 79, 80, 92 | 49, 53, 199 (first) |
| dirty_pr | 10 | - | 6 | 58, 60, 64 | 54, 54, 257 (first) |
| pr_batch_response | 8 | - | 4 | 44, 45, 57 | 50, 50, 214 (first) |

On `tests/shared_frontier_wide/wide_64.dl6`, where every level is 1 arm, the delta wins: 10-11 us against 18-22 us for the rebuild, on all 64 levels. So the crossover is the ARM COUNT, never a base-table row count.

`page_response`'s first call costs 20.4 ms, which is SQLite planning 248 KB of SQL; the statement cache holds it after that. It is 32.8 ms of a 234 ms ghcache fold in 5 calls.

Not an index problem: `DL_EXPLAIN=1 RUST_LOG=sprefa_engine_rs::explain=info` over the whole fold explains 1,232 distinct statements and reports `scan=true` on zero of them.

Two candidate closes, both needing the user:
1. Emit N arms rather than 2^N. The subset expansion is only needed if an arm must not double-count a row derivable from two moved rels at once, and `INSERT OR IGNORE` plus `SELECT DISTINCT` already handle that (`(delta a) JOIN b` and `a JOIN (delta b)` both produce the `(delta a) JOIN (delta b)` rows; the head is a set).
2. Per-level rebuild-vs-delta, chosen on the arm count at construction. Cheap to decide, but a rebuild produces no delta, so it needs the snapshot-and-diff machinery #427 deleted, which is two engines in one path.

Receipt for closing: ghcache fold wall at or below the pre-#427 152 ms with `tests/fixtures/ghcache_ticklog_base.txt` byte-identical and `grade.sh byte-clean=340`.
