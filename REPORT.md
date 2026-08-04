# Lane L-G: config feeder golden

## Head-of-line check

`git log --oneline -1` at start:

```
1e7b6843 merge lab/ghcacher-plan: ghcacher design vs the OG rust tool (measured 304 golden defect, 22-row expressibility table, 8 lanes in 3 waves)
```

Matches the 1e7b6843 pin. Lane proceeded.

## Deliverable

`v6/tsv2/goldens/ghcacher_config_golden/`:

| file | role |
|---|---|
| `0_ghcacher_config_golden.dl6` | the config-feeder program |
| `1_schedule.json` | hermetic 8-tick arrival schedule |
| `2_expected.tick.jsonl` | exact tick log (oracle) |
| `3_expected.final.jsonl` | exact final envelope |
| `4_oracle.pl` | reference-engine oracle (generic, mirrored) |
| `6_gate.sh` | compile + replay both doors + byte-diff |
| `README.md` | mechanism + graded boundary table |

`REPORT.md` is at the worktree root, as required.

## Design (from `plans/2026-08-04-ghcacher-plan.md` 4.2)

Config search order is DATA: `config_candidate(rank, path)` rows, a host
confirms existence (`config_present`), `min/1` over rank picks the winner
(`best_rank`), `chosen_config` is the path at that rank. Host rows arrive via
the schedule seam (same pattern as `ghcacher_tick_golden`); hermetic, no shell,
no network, no wall clock.

## Two surface deviations, both compiler-forced and documented in README

1. **Host names.** The plan's `sh path_exists(config_path, bucket)` and
   `sh read_org_config(config_path, bucket)` do not compile on the current
   surface: unregistered `sh` names get all-identity roles, so a `bucket`
   freshness salt is impossible (oracle throws
   `template_mismatch(unreferenced_input(bucket))`). They are spelled as the
   registered `repos` (org identity, bucket freshness) and `answer` (name
   identity, bucket freshness) hosts. Rel signatures and compute rules are the
   plan's verbatim.

2. **Bucket latch.** Without a keyed replacement relation, the engine does not
   retract `config_present` when the clock advances (observed in the oracle),
   so graded expectation 2 (the rank-1 row retracting and the choice moving,
   tick-visible) requires the clock golden's own pattern: a `current_bucket
   (period, bucket) key(1)` latch fed from `interval`, which config_present and
   want_org read instead of `interval` directly.

Environment note: the emitted door needs the engine's transitive deps, so
`pnpm install --frozen-lockfile` was also run in `v6/sprefa-store/js` (the
brief names only `v6/tsv2`). Both are gitignored; no tracked-file change.

## Validation (verbatim)

```
bash v6/tsv2/goldens/ghcacher_config_golden/6_gate.sh
```

```
COMPILE-TRACE program=0_ghcacher_config_golden parse=8/74672 plan=5/75239 lower=4/20636 boot=0/434 emit=8/71785 write=1/92 total=26/242858
GHCACHER_CONFIG_GOLDEN_HOLDS ticks=8 final=1
```

```
cd v6 && just conformance && just plunit
```

```
conformance: 293 PASS, 0 FAIL/ERROR
plunit:      [324/324] passed
```

## Graded expectations

| # | expectation | evidence in schedule |
|---|---|---|
| 1 | all four present -> rank-1 path | tick 2: `best_rank=1`, `chosen_config=flag.toml` |
| 2 | rank-1 absent -> rank 2, tick-visible move | tick 4 del: `chosen_config` `flag.toml` cleared, `config_present` rank 1 retracted; tick 5 add: `best_rank=2`, `chosen_config=env.toml` |
| 3 | none present -> empty, no error | tick 8: zero `config_present`/`best_rank`/`chosen_config`/`want_org`, gate exits clean |
| 4 | tick logs byte-identical oracle vs emitted | `6_gate.sh` diffs oracle vs emitted (and both vs expected), all identical |

## Commits

`lab/gh-config`, logical steps, no push, no merge.
