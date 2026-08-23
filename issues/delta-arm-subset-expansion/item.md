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

## Decisions

### 2026-08-23T18:15:50Z · @claude-lane-fix-delta-arm-subset-expansion

DIAGNOSIS CORRECTED, measured at 3b2064aaf. `levels[i].insert_sql` is NOT one arm per subset of the body.

lower.pl:level_positive_delta_arms/9 walks positive body uses ONE at a time. One clause with N positive items yields N arms, the IVM count the issue asks for. Two rails pin it (compile/test/plunit_tests.pl, `delta_arm_count`):

| program | clauses | delta arms |
|---|---|---|
| 4 plain body items, no coalesce | 1 | 4 |
| 1 driver + 3 coalesce | 8 | 20 |

16 arms would be the subset reading of case 1; it is 4.

THE 2^N IS UPSTREAM. 0_coalesce_expand.pl:9 states it: "Multiple coalesce goals produce 2^N clauses." N coalesce goals fan one rule out to 2^N ordinary clauses (present arm = bare atom, absent arm = not(probe) + := default) BEFORE lower.pl runs. Each clause then contributes its own linear arms.

ghcache page_response has 6 coalesce goals (ghcache.dl6:500-509):
- 2^6 = 64 clauses
- 64 recompute_insert_sqls, 72 KB
- sum over subsets of (1 + |S|) = 64 + 6*32 = 256 delta arms, 248 KB
- refCount support_sql 74 arms, 93 KB
- 414 KB of SQL for one head

poll_state: 4 coalesce -> 16 clauses, 48 arms. pull_request_seen: 8 clauses, 16 arms. pr_batch_response: 4 clauses, 8 arms. Every row in the issue's table is 2^(coalesce count), never 2^(body width).

This kills BOTH candidate closes. Candidate 1 (emit N arms) is already what lower.pl does. Candidate 2 (per-level rebuild-vs-delta) does not help: the rebuild is 64 statements, 2^N as well.

THE FIX IS THE USER'S OWN RULING. rulings.pl:479-480 (null_design, user 2026-07-30) names the lowering: "one body operator, LEFT JOIN + coalesce in SQL, `?? default` in rx". ARCH.pl:394-396 records that the implementation went the other way on purpose: "Tier 0 and NOT a new lowering: 0_coalesce_expand.pl (expansion phase 45) rewrites one rule into two ordinary clauses ... so the emitter gained nothing." That trade is what costs 2^N. registry.pl:64-66 pins the current phase order as a rule ("a coalesce reaching the lowering would be a phase-order defect").

Baseline receipts, release emit_rust_harness, ghcache.schedule.json, 3 runs:
- level_insert page_response: 34955 / 34648 / 33116 us over 5 calls, 14.2 / 14.4 / 13.8 pct of the fold
- recount page_response: 5947 us over 50 calls
- ddl: 85174 / 82089 / 83915 us over 1657 calls, 34.7 / 34.0 / 34.9 pct, LARGER than page_response and a separate defect

Landing LEFT JOIN + SQL coalesce needs: 0_coalesce_expand.pl (keep every validation throw, stop rewriting level rules; edge rules keep the latest/1 split so fixtures/7_coalesce.pl case d is untouched), registry.pl (wrapper row), analyze.pl + strat.pl (the source rel needs the STRICT stratum edge the absent arm's not(...) supplies today, per fixtures/7_coalesce.pl:96), engine.pl (oracle solves coalesce/2), lower.pl (11 FROM-assembly sites, all `atomic_list_concat(Parts, ', ', Sql)`). The refCount retraction path already covers the flip fixtures/7_coalesce.pl:77 grades, and level_ref_count_sql is emitted for every non-aggregate head unconditionally (lower.pl:4288), so the non-monotone LEFT JOIN keeps its retraction machinery.

NEEDS THE USER: this reverses the ARCH.pl:394 decision and moves a construct off the shared-expander phase order. Not a lane call.
