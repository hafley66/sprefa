# Lane L-B: tier golden

## Base verify

`git log --oneline -1` = `95193b1b` (matches lane brief). No deviation.

## What this lane built

NEW directory `v6/tsv2/goldens/ghcacher_tier_golden/`:

| file | role |
|---|---|
| `0_ghcacher_tier_golden.dl6` | 4.3 tier program transcribed + a `poll`/`fetch` downstream gated on the `due` row |
| `1_schedule.json` | hermetic schedule, buckets 100/101/102/120/150, host fetch responses on the next tick |
| `2_expected.tick.jsonl` | oracle tick log (6 ticks) |
| `3_expected.final.jsonl` | oracle final relation envelope |
| `4_oracle.pl` | oracle driver (copy of the tick golden's) |
| `6_gate.sh` | two-door byte diff + COUNT leg + slug-list/points-budget grep |
| `README.md` | schedule table, measured values, sabotage receipt |

`REPORT.md` at worktree root (this file).

## Graded expectations

1. Tick logs byte-identical oracle vs emitted, both doors. `diff` on ticks line
   1-6 and the final line 1 passes for oracle and emitted; oracle vs emitted
   passes.
2. COUNT leg in `6_gate.sh`: extracts each tick line's `poll` rows, asserts the
   cold repo appears exactly on its due buckets (120, 150), the hot repo on
   every clock tick, and a non-due cold bucket contributes ZERO `poll` rows.
   Counts are printed.
3. Slug-list aggregation row `[120, 0, 'org/cold org/hot']` and
   `points_budget` `[120, 1]` match the plan's measured 5.3 value, asserted by
   grep in the gate.

Sabotage check ran: changing the cold `period_ticks` from 30 to 1 makes the
cold repo fire every tick and the gate exits 1 on the byte diff. Reverted
afterward; gate passes green again.

## Validation (paste verbatim)

```bash
bash v6/tsv2/goldens/ghcacher_tier_golden/6_gate.sh
```

```text
cold poll rows total=2 (due buckets 120,150 only): OK
hot poll rows total=5 (every clock tick): OK
GHCACHER_TIER_GOLDEN_HOLDS ticks=6 final=1
```

```bash
cd v6 && just conformance && just plunit
```

```text
just conformance: 294 PASS, 0 FAIL
just plunit:      324/324 passed
```
