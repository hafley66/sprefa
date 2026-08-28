# lab-events-firehose-pro4 (pass 1 of 2; a coordinator design review follows) [pro4 arm: identical brief, second model; suffix every PR title with " (pro4)"]

You are lane `lab-events-firehose-pro4`. Coordinator is `sprefa-coordinator`.
Base sha 9e4b468157bb2a189960b8ec69daad10af372862. Branch `lab/events-firehose-pro4`.
FIRST ACTION: `git merge --ff-only 9e4b468157bb2a189960b8ec69daad10af372862`; on failure STOP and hail.

## Defect (PR #439 audit, "one more finding, not fixed")
`v6/dl/ghcache/ghcache.dl6` polls `users/<owner>/events` (kind `user_events`) and `users/<me>/events/orgs/<owner>` (kind `org_events`) every bucket (rules at ghcache.dl6:284-299), and NOTHING reads the answer. `repo_event_seen` (ghcache.dl6:742-748) joins `watched_endpoint(... endpoint_kind: 'repo_events' ...)` only. 1,435 calls a day are fetched and dropped.

## Deliverable: one new rule arm, program only
Add a second `<-` arm to `repo_event_seen` that consumes the user/org events firehose and routes each event to the repo it names. GitHub event items carry `repo: {name: "<owner>/<name>"}`. Shape (adapt variable names to the file's conventions; descriptive names, never single letters):
```
repo_event_seen(RepoRef, GhId, EventType, Actor, Payload, CreatedAt) <-
  watched_global(endpoint_path: EndpointPath, endpoint_kind: EndpointKind),
  EndpointKind == 'user_events',
  rest_response(endpoint_path: EndpointPath, status: RespStatus, body: Body),
  RespStatus == 200,
  decode(Body, [... {id: GhId: text, type: EventType: text,
                     actor: {login: Actor: text},
                     repo: {name: RepoSlug: text},
                     payload: Payload, created_at: CreatedAt: text}]),
  <split RepoSlug on "/" into Owner, Name>,
  watched_repo(repo(Owner, Name), Owner, Name, ...),
  RepoRef := repo(Owner, Name).
```
Do the same for `org_events`, or fold both kinds into one arm with a `match` on `EndpointKind` if the file already uses `match` for a sibling (it does at :265 and :287; follow that shape). Constraints:
- `split/2` is registered (`v6/prolog/compile/registry.pl:294`); `substr/2,3` at `:285-286`. Check the manifest `v6/prolog/compile/out/manifest.json` for a fixture that uses `split` to copy its exact spelling before writing yours. If no fixture spells `split` into two bound variables, use two `substr` calls around `instr`-style logic only if a fixture shows it; otherwise STOP and hail: "split-to-two-vars has no fixture spelling".
- Only events whose repo is in `watched_repo` land; unknown repos are dropped silently (that is the `watched_repo` join).
- The keyed fold on `gh_id` de-duplicates an event that arrives from BOTH the repo poll and the firehose (comment at :740-741 explains why an identical write is a zero delta). State in the PR body that this is the dedup and cite that comment.
- `dirty_pr` and everything downstream must not change.

## Receipts
- `bash v6/dl/ghcache/gate.sh` must print `GHCACHE_RUST_DOOR_HOLDS ticks=14 account_ticks=14` and goldens 6 (background, `timeout 900`, tail the log). If the account schedule (`ghcache.account.schedule.json`, built by `build_account_schedule.py`) has a `user_events` 200 response, the new arm fires on it: add a COUNT assertion to gate.sh (a `jq` line in the style of the existing `fresh=`/`cached=` lines) that `repo_event_seen` rows include at least one from the firehose (say how you tell them apart; if you cannot, add a scripted 200 response for `users/hafley66/events` with one event naming `hafley66/sprefa` to the account schedule builder and say exactly which lines changed).
- Compile clean: the gate's compile step.
- Full gate numbers in the PR body: conformance 445/0 (`cd v6/prolog/conformance && swipl -g go -t halt go.pl`), plunit (`cd v6 && just plunit`) /0, grade `bash v6/sprefa-engine-rs/grade.sh` 445/341.

## Yield results over time (mandatory)
1. after the arm compiles: `boop beep hail sprefa-coordinator --from lab-events-firehose-pro4 --body "compiles: arm shape <one line>"`
2. after gate.sh holds: hail ticks/goldens numbers + firehose row count.
3. done: PR number + full gate.
STOP and hail on: split spelling missing, gate.sh compile stop naming your arm (paste the `"code"` line), or a schedule change wider than one response.

## You own
`v6/dl/ghcache/ghcache.dl6`, `v6/dl/ghcache/gate.sh`, `v6/dl/ghcache/build_account_schedule.py`, `v6/dl/ghcache/ghcache.account.schedule.json`. Forbidden: v6/prolog/**, v6/sprefa-engine-rs/**, other v6/dl programs.

## Style laws (CLAUDE.md)
rxjs/prolog/SQL vocabulary only; no em dashes; banned words: provenance, substrate, load-bearing, regime, ground truth (oracle), refusal, support (refCount), honest. Comments state only constraints the code cannot show. dl variable names descriptive. Commit per deliverable; PUSH before reporting. PR title: `ghcache: the user/org events firehose feeds repo_event_seen`.
