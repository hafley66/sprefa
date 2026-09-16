# oracle-grind REPORT

Directive: every part of the reference prolog disabled, skipped, or fixed;
never run the whole thing; grind the slowest down. Base 742e177a9; two
commits landed by the stood-down opus predecessor, finished by the Fable lane.

## What landed

| piece | where |
|---|---|
| per-fixture alarm budget, default 10s, env `SWEEP_ORACLE_FIXTURE_BUDGET_S`; a capped fixture prints `ORACLE_CAPPED <name>` and the stage continues | `compile/oracle_dump.pl` (6f8ce297c) |
| `out/oracle.timings.tsv` (fixture, ms, capped) + top-10 print per forced pass | same commit |
| top offender ground: `level_round_cap` 1000 -> 50, fixture expectation updated, error shape identical | `conformance/level_eval.pl`, `fixtures/23_diverging_recursion.pl` (ae29b4501) |
| `SWEEP_ORACLE=0`: stage 2 never runs, stage 3 diffs the committed snapshots, prints `SWEEP_ORACLE=off` | `v6/tsv2/scripts/sweep.sh` (this commit) |

## Measurements

One forced oracle-only timing pass (the only full pass run; machine carried
4 foreign swipl processes at ~100%, load noted):

| measure | before | after |
|---|---|---|
| oracle stage total, 462 fixtures | 4499 ms | 2010 ms |
| slowest fixture | `diverging_measure_recursion_is_bounded_and_loud` 3512 ms | same fixture 13 ms |
| fixtures over 1s | 1 | 0 |
| new slowest | - | `nested_list_text_door` 75 ms |

Gate passes on this tree (commands as spelled):

```
SWEEP_ORACLE=0 SWEEP_JOBS=8 bash scripts/sweep.sh   # no-change: real 1.8s
  SWEEP_CACHE hit=461 recompiled=0 / SWEEP_ORACLE=off / FINAL ... final_wrong=39
SWEEP_JOBS=8 bash scripts/sweep.sh                  # no-change, oracle on: real 2.1s
  ORACLE_CACHE hit=462 redumped=0 capped=0 / FINAL ... final_wrong=39
cd v6/prolog/conformance && swipl -g go -t halt go.pl   # FAILURES 1 (standing known-red)
```

`final_wrong=39` is the standing pre-existing set (enum-plane arrivals,
CI-KNOWN-RED), byte-stable across every pass above. Snapshot churn: zero
files beyond the one edited fixture's expectation (ae29b4501); `git status`
clean after each pass.

## Skips

None. Nothing needed `ORACLE_SKIP`: after the cap grind every fixture's
oracle wall is under 100 ms under load. The budget and the off switch stand
guard for anything future.
