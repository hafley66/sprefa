# feature/signed-delta-round2: per-round refcount, no survivor re-walk

## Ruled by user 2026-08-10: respawn round 2 shaped by the WITH RECURSIVE
## probe. The probe landed (PR #155); its verdicts BIND your design:
## - WITH RECURSIVE owns signed survivor reachability: one distinct
##   recursive walk + frontier clear + weight publish = 3 statements
##   (v6/sprefa-store/PROBE-REPORT.md, examples/recursive_probe.rs).
## - A CTE canNOT own the round column: the accumulated-set guard needs a
##   second recursive reference (SQLite: `multiple recursive references`),
##   and keying (round,key) defeats UNION dedup on cycles. Round/refcount
##   mutation stays in the rust loop.
## - Incremental folding removes the whole-row refcount refill; the
##   remaining cost is per-round staging (27 dispatches in round 1).

## The goal
Beat banked round 1: retract_signed_delta at 27 statements / 1669.7ms
(perf_report row, PERF-REPORT.md signed-delta appendix). Fold the per-round
refcount so survivors are never re-walked: combine the probe's 3-statement
recursive survivor walk with rust-side per-round refcount folding. DRed
sibling (53 stmts / 1753.4ms) and dd (175.4ms) stay the comparison rows.

## Prior art, all in tree or banked
- v6/sprefa-store/src/engine.rs: retract_signed_delta (round 1, merged #146)
  and the probe's recursive walk (merged #155)
- v6/sprefa-store/tests/agreement.rs: DAG + cyclic matrix; round 2 MUST
  keep agreement 4/4 including cyclic
- sprefa-lanes/signed-delta-round2-3red.patch + -cone-test.rs: the dead
  first try at per-round refcount, 3 red tests; reference for what failed
- Repo skills BEFORE any schema/statement design:
  .claude/skills/sql-relational-design, .claude/skills/sqlite-costs

## Deliverables
1. retract_signed_delta_v2 (or evolve v1 in place; say which and why in the
   commit message) with the folded per-round refcount.
2. Agreement tests green for it across the existing matrix + one new case
   that killed the 3-red attempt (read the banked cone test).
3. perf_report bench row; PERF-REPORT.md appendix updated with the
   statement count + ms beside round 1 / DRed / dd.
4. COUNT receipts: statement count per retraction asserted in a test
   (formerly-quadratic law: counts, never end-state equality alone).

## Files you own
v6/sprefa-store/ only (src/engine.rs, src/lib.rs, tests/, examples/,
PERF-REPORT.md).

## Setup
```bash
cd <worktree>/v6/sprefa-store && cargo build
```

## Gate
```bash
cd <worktree>/v6/sprefa-store && cargo test && cargo run --release --example perf_report
```
All green; paste the perf row in the final commit message.

## Rails
- NEVER git merge / pull / rebase in the worktree.
- Blocked -> FAILURE-REPORT-ROUND2.md, exact command + output, exit
  NONZERO. rc=0 with a dirty tree or red gates is a defect.
- NEVER --no-verify. Comment budget: max 2 consecutive comment lines.

## Style
Comments state only constraints the code cannot show. Banned words, prose
and identifiers: provenance, substrate, load-bearing, regime, refusal.
