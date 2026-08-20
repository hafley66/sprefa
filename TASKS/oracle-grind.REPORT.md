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
| oracle OFF BY DEFAULT (user 2026-08-20 "default the prolog sweep to false bc the conformance prolog is literally not my product"; rulings.pl `oracle_demoted_to_snapshots` + same-day amendment): stage 2 runs only under `SWEEP_ORACLE=1`; default prints `oracle=off snapshots=<date> (<newest>)` and diffs frozen snapshots | `v6/tsv2/scripts/sweep.sh` |
| a compiled fixture with NO snapshot fails the default pass by name: `SNAPSHOT MISSING <name>: mint it with SWEEP_ORACLE=1`; a THROWING fixture's verdict persists as `<name>.oracle.throw` (85 minted) so absence stays meaningful | `oracle_dump.pl` marker + the sweep.sh check |

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
SWEEP_JOBS=8 bash scripts/sweep.sh                  # DEFAULT, oracle off: real 2.1s
  oracle=off snapshots=2026-08-20 (tightened_baseline_catches_regrowth.oracle.jsonl)
  FINAL ... final_wrong=39
SWEEP_ORACLE=1 SWEEP_FORCE=1 SWEEP_JOBS=8 bash scripts/sweep.sh   # mint/refresh pass
  85 ORACLE_THROW markers written; every tracked .oracle.jsonl byte-identical
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
