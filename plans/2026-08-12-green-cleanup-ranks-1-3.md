# 2026-08-12 green-cleanup ranks 1-3

## One sentence

Three `green-all` legs (`flagship`, `getting-started`, `scale-floor`) fail
because a pinned expectation went stale, not because anything is broken;
refresh each pin and delete its row from the known-red ledger.

## Ownership

Ranked cheapest-first from `plans/2026-08-12-green-all-triage.PLAN.md` section
5. This lane owns ranks 1-3 only.

| rank | leg | fix |
|---|---|---|
| 1 | `flagship` | regenerate the v5 golden with `FLAGSHIP_V5_WRITE=1` |
| 2 | `getting-started` | update doc block 24 to the new `rule-index unavailable:` text |
| 3 | `scale-floor` | re-pin expected stmts set to `[39,43]` with a reason |

## Files owned

| path | permission |
|---|---|
| `v6/tsv2/scripts/flagship-callgraph.sh` + golden | full |
| `v6/GETTING-STARTED.md` | full |
| `v6/tsv2/scripts/7_scale-floor.sh` | full |
| `.github/CI-KNOWN-RED.md` | delete only rows turned green |
| `plans/2026-08-12-green-cleanup-ranks-1-3.md` | create |

Forbidden (other lanes own): `v6/prolog/lower.pl`, `analyze.pl`, `compile.pl`,
`0_generic_expand.pl`, `compile/6_emit_dd_plan.pl`, `emit_rust.pl`,
`v6/sprefa-engine-rs/**`, `labs/break-hunt/**`, `test/plunit_tests.pl`,
`v6/tsv2/labs/1_rtkq-extraction-golden.ts`, `print_dl.pl`.

## Gate

Each leg passes 3 times in a row, one at a time, from `v6`:

```bash
just flagship
just getting-started
just scale-floor
```

`scale-floor` is timing sensitive; checked `boop beep ps` before the gate and
waited. All lanes were <=0.1% cpu during the scale-floor runs.

## Rank 3 note

The triage lane proved the set is flat [`39,43`] at both 1k and 10k, so
delta-proportionality holds and the pin is stale by a constant +2. Re-pin only
after determining why the number moved; a pin refreshed with no explanation is
how a regression gets absorbed into a baseline. If the two extra statements per
tick are a real cost regression, stop and report instead of re-pinning.
