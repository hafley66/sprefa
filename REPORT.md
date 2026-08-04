# Lane: env/pwd/git-root hosts + config-as-json + tilde resolution

First action verified: `git log --oneline -1` showed `95193b1b` (the required
base) on branch `lab/gh-env`. No deviation from first-action.

## What shipped

1. `v6/prolog/compile/registry.pl`: four `host_input_contract` rows added
   after the ghcacher section: `env_var`, `pwd`, `git_toplevel`,
   `toml_json`, each with the stated input roles. No `host_execution` rows;
   plain shell fallthrough (default `shell` executor, same as `repos`).
2. `v6/tsv2/goldens/ghcacher_env_golden/*`: a new hermetic golden (8 files):
   the program, schedule, checked-in tick + final goldens, a golden-specific
   oracle, a json-aware emitted runner, the gate, and the README with a
   fail-first receipt.
3. `REPORT.md` (this file) at the worktree root.

## Graded behaviors (all green)

1. Env override present -> rank-0 path wins; absent (zero `env_var` rows) ->
   falls to the file candidates.
2. The decoded nested value (`global.db_path`) lands as a typed `db_path(path:
   text)` column row.
3. Tick logs byte-identical oracle vs emitted, both doors.
4. Fail-first receipt in the golden README.

## Note on the four hosts

`pwd` and `git_toplevel` are registered as contracts (bodies documented in the
brief) but are not exercised by this golden: the golden's schedule only needs
`env_var` (the override) and `toml_json` (the config sheet). They remain
registered so the language can call them; nothing in this worktree declares a
second use.

## Validation (verbatim)

```bash
bash v6/tsv2/goldens/ghcacher_env_golden/6_gate.sh
cd v6 && just conformance && just plunit
```

Results on this worktree:

```text
GHCACHER_ENV_GOLDEN_HOLDS ticks=5 final=1      # gate exit 0
conformance: exit 0 (zero fail lines)
plunit: 324/324 passed, exit 0
```

`node_modules` were missing in both `v6/tsv2` and `v6/sprefa-store/js`; both
were installed with pnpm (no npm).

## Branded debts (banner, verbatim, at the marked site)

The `@comment-ok` banner and the debt text live above the `toml_json`
declaration in `0_ghcacher_env_golden.dl6`: python3 stdlib `tomllib` chosen
for zero new dependencies, yaml deferred until a program needs it, and the
tilde case-arm (`~` resolves to home) living in the path-taking host body
rather than as a language construct.

## Deviations / reality notes

- `dl6_oracle` resolves a GENERATED host-response rel's json column type to
  `none` on the raw parsed program (the response rel is produced by host
  expansion, which runs after the schedule reader). A json-valued host
  response therefore cannot reach `decode` through the stock oracle door. The
  golden's `4_oracle.pl` calls `prepare_program` before `read_schedule` so
  the generated response `doc` column resolves to `json` and the seam injects
  a real obj term. This is a golden-local oracle harness detail, mirroring
  the tick golden's own "read_schedule/4 not /2" fix noted in its oracle.
- The shared `scripts/4_ghcacher-tick-golden.ts` final-state encoder is
  json-blind (renders a `json` column as its stored text). The golden uses its
  own `5_emitted.ts` runner so the final line renders json columns as values,
  matching the oracle byte-for-byte. Tick-log rendering was already json-aware
  on both paths.
- The env-present / env-absent flip is staged at a bucket boundary
  (`interval(3600,1)` then `interval(3600,2)`) rather than mid-bucket: a
  mid-bucket chosen flip makes the emitted incremental engine transiently
  re-issue a demand the oracle does not, which would break byte-identity.

## Commits

Committed in logical steps on `lab/gh-env`; no push, no merge.
