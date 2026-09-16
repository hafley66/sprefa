# feature/sqlite-recursive-probe

## RESPAWN NOTE (2026-08-11): attempt 1 stalled 5+ hours with staged work and
## no commit. Its full diff (PROBE-REPORT.md draft, examples/recursive_probe.rs,
## engine.rs/lib.rs/agreement.rs edits) is banked at
## sprefa-lanes/probe-attempt1.patch (667 lines) — it was cut against base
## 8676a752; your base is newer (that branch merged as PR #146), so apply
## hunks selectively or use it as reference only. Finishing means COMMITTING
## green work or a failure report with nonzero exit.: fewer statement crossings, less b-tree churn

## Base
Branch from 8676a752 (feature/signed-delta-retraction head: retract_signed_delta,
agreement tests, banked bench numbers). ADDITIVE ONLY: never modify
retract_dred/retract_scc/retract_signed_delta; new probe code goes in
examples/recursive_probe.rs plus (if needed) new engine methods.

## Question 1: WITH RECURSIVE as the round collapser
Today every retraction round crosses the rust->sqlite boundary as its own
prepared-statement dispatch (banked: dred 53 statements, signed-delta 27, for
one 960k retraction). Probe whether recursive CTEs collapse passes into ONE
VM entry each:
- over-delete cone as `WITH RECURSIVE cone(key) AS (...)` feeding ONE
  set-based DELETE/UPDATE apply;
- rederive set the same way;
- the signed-delta round loop with the ROUND as a CTE column (the CTE climbs
  rounds inside one statement). Known wall to characterize honestly: a
  recursive CTE cannot UPDATE refcounts mid-recursion; state exactly which
  half stays outside the CTE and why.

## Question 2: less b-tree work, folded consolidation
Append-only delta tables instead of DELETE+refill, with consolidation as an
INCREMENTAL fold each round (user preference 2026-08-10: folded over time,
NOT a periodic GROUP BY/HAVING sweep). Measure b-tree write volume via
statement counts and sqlite3_status if reachable.

## Keys law (already satisfied, keep it that way)
All tables INTEGER-keyed (engine.rs:116-128, key INTEGER PRIMARY KEY, virtual
tag/id via arithmetic). Any TEXT key in probe DDL is a defect.

## Receipts required
- Bench table on the SAME 960k DAG + cyclic fixtures against the banked
  numbers: dred 1753.4ms/53stmt, signed-delta 1669.7ms/27stmt, dd 175.4ms.
- Statement counts per variant (COUNT law: counts, never wall alone).
- Byte-identical survivor sets vs the oracle for every variant (reuse
  tests/agreement.rs harness; add the probe variants to the matrix).
- PROBE-REPORT.md at v6/sprefa-store/ summarizing: what a recursive CTE can
  and cannot own, wall + stmt table, whether the representation gap moved.
- Every bench step under 10s.

## Commit rail (commit-or-report)
Commit ON THE BRANCH feature/sqlite-recursive-probe, up to 2 commits, prefix
`rust:`. Blocked -> FAILURE-REPORT.md, exact command + output, exit nonzero.
NEVER --no-verify.

## Style
No eprintln! in src/**. Comments only constraints code cannot show, max 2
consecutive lines. Banned words prose+identifiers: provenance, substrate,
load-bearing, regime, refusal. Read .claude/skills/sqlite-costs before DDL.
