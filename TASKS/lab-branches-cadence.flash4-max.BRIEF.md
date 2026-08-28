# lab-branches-cadence (pass 1 of 2; a coordinator design review follows)

You are lane `lab-branches-cadence`. Coordinator is `sprefa-coordinator`.
Base sha 9e4b468157bb2a189960b8ec69daad10af372862. Branch `lab/branches-cadence`.
FIRST ACTION: `git merge --ff-only 9e4b468157bb2a189960b8ec69daad10af372862`; on failure STOP and hail.

## Goal (PR #439 audit row, listed not landed)
`repos/<owner>/<name>/branches?per_page=100` polls every bucket (60 s) per watched repo: 1,440 calls/day/repo. Make its period a config key with the SAME plumbing `org_repo_discovery_interval_seconds` already has. Default stays 60 s (no behavior change without config), so goldens stay byte-identical.

## Exact changes, `v6/dl/ghcache/ghcache.dl6` only
1. `rel global_setting(...)` at :33-36: add a column `branches_period: int` (place it after `org_discovery_period`). It is `key(1)`; keep that.
2. The `global_setting(...) <+ chosen_config(Doc), decode(Doc, {global: {...}})` rule at :113-123: decode a new key `branches_poll_interval_seconds: BranchesPeriod: int`. If the config doc lacks the key the decode yields no row, so use the file's existing default mechanism: grep the file for how a missing optional config key is defaulted (`coalesce(` at :661 is the idiom for a missing row; `option(` and `default` are other spellings). If the config decode has NO optional-key idiom anywhere in the file, STOP and hail "global_setting has no optional-key idiom; adding branches_poll_interval_seconds to the sample config is the alternative" and wait.
3. Add one `period_candidate` arm next to the `org_repos`/`user_repos` arms at :325-336, same shape, for `endpoint_kind: 'branches'`, reading `global_setting(branches_period: BranchesPeriod)`. The existing first arm at :316-323 excludes `org_repos`/`user_repos` by `!=`; add `EndpointKind != 'branches'` there so the branches endpoint does not also take the general `PollPeriod` (`endpoint_period` is `max(...)` at :354, so a stray general candidate would be harmless only when the branches period is the larger; make the exclusion explicit anyway and say why in the PR body).
4. Update the README/sample config wherever `org_repo_discovery_interval_seconds` is documented (grep `v6/dl/ghcache/README.md` and any `*.toml`/`*.json` sample under `v6/dl/ghcache/`) with the new key and its default 60.

## Receipts
- `bash v6/dl/ghcache/gate.sh`: `GHCACHE_RUST_DOOR_HOLDS ticks=14 account_ticks=14`, goldens 6, byte-identical fold output vs base (run gate.sh on base first, save its `out`, diff). Background with `timeout 900`; never foreground-wait over 10 s.
- A COUNT receipt: with the schedule's config carrying `branches_poll_interval_seconds: 900`, the branches endpoint appears in `call_log` at most once per 15 buckets. If the scripted schedule cannot carry a config override, say so and give the arithmetic instead (period_ticks = ceil(900/60) = 15) as the PR-body receipt; do not invent a schedule mechanism.
- Full gate numbers in the PR body: conformance 445/0, plunit /0, grade 445/341.

## Yield results over time (mandatory)
1. after step 2 resolves (idiom found or STOP): `boop beep hail sprefa-coordinator --from lab-branches-cadence --body "optional-key idiom: <what>"`
2. after gate.sh holds byte-identical: hail the numbers.
3. done: PR number + full gate.

## You own
`v6/dl/ghcache/ghcache.dl6`, `v6/dl/ghcache/README.md`, sample config files under `v6/dl/ghcache/`. Forbidden: v6/prolog/**, v6/sprefa-engine-rs/**, gate.sh, schedules, other v6/dl programs.

## Style laws (CLAUDE.md)
rxjs/prolog/SQL vocabulary only; no em dashes; banned words: provenance, substrate, load-bearing, regime, ground truth (oracle), refusal, support (refCount), honest. dl variable names descriptive. Comments only for constraints the code cannot show. Commit per deliverable; PUSH before reporting. PR title: `ghcache: branches poll period is a config key, default unchanged`.
