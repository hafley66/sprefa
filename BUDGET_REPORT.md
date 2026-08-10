# BUDGET_REPORT: dl6-bench-budget cell

Budgeted bench gate for the battery: the `dl6` grid_10000 bench runs under
TIME and RSS ceilings and exits nonzero on breach. Closes failure-modes rail-gap
entry 45 (docs/failure-modes.md), the correctness-only gate class that let a
4.3x perf cliff with identical checksums ride 25 green PRs.

## What was built

| file | role |
|---|---|
| `v6/labs/exec_shootout/dl6/budget.json` | ceilings for `grid_10000`: `fixpoint_ms_ceiling 2500`, `peak_rss_mb_ceiling 900` |
| `v6/labs/exec_shootout/dl6/budget-check.sh` | runs grid bench, compares FACTS.json against budget.json, exit 2 on any breach; ratchet law in the header |
| `v6/justfile` | recipe `dl6-budget` + appended to `green-all-serial` |

budget.json has no comment support, so the ratchet law lives in the
budget-check.sh header: ceilings move only DOWN, or with a written receipt in
the commit message. They never loosen silently.

The check runs the bench under `v6/tools/run-capped.sh` (failure-modes class
38) so a hung bench is killed, not waited on. Metrics come from FACTS.json as
written by bench.ts's `DL6_BENCH_JSON` writer: `cases[].fixpointMs` and
`cases[].peakRssKb` (div 1024 to MB).

A skip override (`DL6_BUDGET_SKIP_BENCH=1`, `DL6_BUDGET_FACTS=path`) lets the
comparison grade a specific FACTS file without rerunning the bench, used for
the fail-pre-fix receipt below.

## FAIL-PRE-FIX RECEIPT (doctored incident numbers)

Incident numbers: fixpoint 5627ms, RSS 1364MB. Check exits 2, both metrics
breach.

```
$ DL6_BUDGET_SKIP_BENCH=1 DL6_BUDGET_FACTS=/tmp/dl6-doctored-facts.json bash v6/labs/exec_shootout/dl6/budget-check.sh
budget grid_10000 fixpoint_ms 5627 <= 2500 BREACH
budget grid_10000 peak_rss_mb 1364 <= 900 BREACH
EXIT=2
```

## REAL RUN (passing)

`just dl6-budget` from `v6/`. Grid run only, within budget on both metrics.

```
$ just dl6-budget
dl6-bench: compiled in 250ms
dl6-bench: grid_10000
budget grid_10000 fixpoint_ms 1242 <= 2500 OK
budget grid_10000 peak_rss_mb 606 <= 900 OK
EXIT=0
```

Measured: fixpoint 1242ms, peak RSS 606MB; grid checksum
`9d7239568960d6a8` (identical to the healthy banked value), derived 1,069,200.

## Commit sha

`fe2ee5c2` on branch `chore/dl6-bench-budget` (no push, no PR).
