---
created: 2026-08-23
updated: 2026-08-23
type: improvement
status: open
priority: high
related: ['@one-path-busy-tick-cost', '@incremental-empty-delta-skip']
labels: [engine, performance]
---

# recount runs on additive ticks: gate the from-base re-derive on a source losing rows, and settle the level plane inside the tick

## Description

_Source: v6/sprefa-engine-rs/src/incremental.rs `reconcile_ref_count_statement` + its two callers_

## Description

Built and measured on `fix/one-path-busy-tick-cost` 2026-08-23, then reverted; the working tree is at `/tmp/shrink_gate_version.rs` on that lane's box and the design is below.

After the level clock gate, `recount` is still 5,630 of the ghcache fold's 9,860 statements. It is a from-scratch re-derive: `support_sql[1]` rebuilds `__support_next` from the base tables, `[2]` overwrites `__refcount` on every head row, `[3]`/`[4]` retract what fell to zero, `[6]`/`[7]` add what is newly derivable. `__refcount` is a pure function of the base tables (nothing outside `support_sql` reads it, verified over the whole emitted IR), so it carries no state across ticks and a tick that skips the recount costs the next one nothing.

In positive datalog a head only LOSES rows when a positive body rel loses rows or a negated one gains them. Gating on that, plus a fourth probe column per rel (`EXISTS(delta WHERE _sign = -1)`, seeding the previous tick's unabsorbed retractions), measured:

| | statements | recount stmts |
|---|---|---|
| clock gate only (landed) | 9,860 | 5,630 |
| plus the shrink gate | 5,944 | ~1,500 |

Negation is readable from the emitted SQL: 9 of ghcache's 100 levels carry `NOT EXISTS` in `insert_sql`, and the rels inside those balanced groups are exactly the negated body items.

**Why it was not landed.** The tick log moved, and the diff is convergence lag, not a wrong final state: on tick 5 the `pr_batch_member -> pr_batch_alias -> pr_selection -> pr_query -> pr_post_field -> __host_demand_http__post` chain reports `add 1, del 1` where the base reports `del 1`, and the same rows retract again on ticks 6 and 7. The cause is that `recompute_levels_before_edges` and `recompute_levels_after_edges` each run their statements ONCE in emitted order. A head that ran early in a pass never sees a source that shrank late in the same pass; today's code hides that by re-running every head in both passes, so the second pass is doing double duty as a fixpoint round.

So the real shape is: run the recompute pass to a fixpoint (`sequence_level_rounds`' round loop, over the whole pass rather than over a recursion group), and then the shrink gate is exact and the two fixed passes stop being load-bearing for convergence. That changes what a tick's delta stream looks like for a chain that settles in one tick instead of three, which is a level-plane semantics call and needs the user in the room.

Receipts for closing: ghcache fold statements at or below 6,000, `tests/fixtures/ghcache_ticklog_base.txt` regenerated with the reason recorded, conformance 444/0, `grade.sh byte-clean=340`.
