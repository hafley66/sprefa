# Brief: delete the dep_crawl executor and the FamilyMemo caches; soopy wrappers stay thin

Base sha: the spawner prints it (main 1e39b557874ff790b2a2d8d0591e134216eda558).

## The user's decision (2026-08-22)
Executors transport; the program owns state and traversal. "Files and git" stay as
executors because they wrap the user's own `soopy` crate; the caches and the crawl on
top of them were added today (commit `456162553`) and go.

## Facts (verified)
- No `.dl6` in the repo references `soopy.dep_crawl`, `soopy.repo_at`, `soopy.refs`
  or `soopy.history` (`grep -rlE '/(dep_crawl|repo_at|refs|history)\(' --include=*.dl6 .`
  is empty at base). `v6/dl/crosswalk/gate.sh` is 10/10 at base; confirm what it
  exercises before you delete anything and keep it 10/10.
- `src/executors/mod.rs:100-133` `FamilyMemo<T>` is the per-process cache used by
  `dep_crawl.rs:37-46`, `git_history.rs:43-47`, `git_refs.rs:23-26`,
  `repo_at.rs:34-38`. `hosts.rs:66-77` holds the `LazyLock` singletons.
- `dep_crawl.rs:104-137` aggregates one traversal into 4 rel shapes; the traversal
  frontier is program work (a rel of repos to visit, closure by rule over
  `soopy.refs`/manifest reads).

## Build this
1. Delete `src/executors/dep_crawl.rs`, its roster row in `hosts.rs` (`LINKED_EXECUTORS`
   and `executor_for`) and in `registry.pl` (roster row only), its tests
   (`tests/revision_walk.rs` and others: grep `dep_crawl`), and the
   `v6/dl/fixtures`/goldens that only it served (list each in the PR).
2. Remove `FamilyMemo` from `git_refs.rs`, `git_history.rs`, `repo_at.rs`, then
   delete it from `mod.rs`. Each executor answers its rows from soopy on every call;
   the engine's per-tick demand dedup (`hosts.rs` `collect`, claimed demands) is the
   only "once per tick" the system needs. If a gate gets slower, measure it (three runs,
   numbers in the PR) and state the delta; do not re-add a cache.
3. If `v6/dl/crosswalk/crosswalk.dl6` or its fixtures need a crawl, write the frontier
   as rules in that program (`repo_to_visit`, closure over manifest `path` deps read by
   `soopy.repo_at`), prove the gate 10/10 and the row counts equal to base.
4. `docs/failure-modes.md` entry: "a lane ported a traversal into an executor cache".

## Ownership
Yours: `src/executors/{dep_crawl,git_refs,git_history,repo_at,mod}.rs` (mod.rs: only the
FamilyMemo block and the dep_crawl export), `src/hosts.rs` ONLY the dep_crawl rows and
the four `LazyLock` statics at `:66-77`, `tests/revision_walk.rs` and any test naming
dep_crawl, `v6/dl/crosswalk/**`, `registry.pl` ONE roster row, `docs/failure-modes.md`
(append). FORBIDDEN: `fetch.rs pulls.rs repos.rs graphql.rs http.rs` and the `collect`
function in `hosts.rs` (another lane), `v6/dl/ghcache/** v6/dl/prwatch/** v6/dl/ghcacher/**`,
every other `v6/prolog/**` file, every other `.dl6`, `v6/tsv2/**`.

## Gate (print every number in the PR body)
- `cd v6/prolog/conformance && swipl -g go -t halt go.pl` (440 PASS at base; never shrinks)
- `cd v6 && just plunit` (1042/0 at base)
- `bash v6/sprefa-engine-rs/grade.sh` (graded=440 byte-clean=335 at base)
- `cd v6/sprefa-engine-rs && cargo test` (175/0 at base)
- `cd v6 && just ghcacher-rust` (goldens=6 at base)
- `bash v6/dl/ghcache/gate.sh` (GHCACHE_RUST_DOOR_HOLDS ticks=13 at base)
- `bash v6/dl/crosswalk/gate.sh` (10/10 at base)
- `swipl -g go -t halt v6/prolog/ARCH.pl`
`export CARGO_BUILD_JOBS=3 RUST_TEST_THREADS=4`. `timeout` on every command. Nothing
foreground over 10s: background it and poll with an `until` loop. Measure a failing leg
three times before calling it broken; read `.github/CI-KNOWN-RED.md` first.

## Laws
FIRST ACTIONS: `git merge --ff-only <base sha>`, then `bash v6/tools/doctor-deps.sh` (DEPS OK
for both crates). Failure = STOP and hail. Never spawn subagents. Commit every green step.
PR against `main`; the PR body carries every gate number and every receipt. `v6/tsv2/**` is
paused: never edit it; emitted TS for an unchanged program stays byte-identical.
No em dashes. Banned in prose and identifiers: provenance, substrate, load-bearing, regime,
refusal, "ground truth" (say oracle). Comments state constraints only; no change-log
narrative, no dates, no PR numbers in comments. dl variable names descriptive, never
single-letter. Surrogate INTEGER keys; no composite TEXT keys. One failure-ledger entry in
`docs/failure-modes.md` per incident this arc fixes (incident, RCA, fail-pre-fix test, rail).
Language design is NOT yours: where this brief leaves a design fork open, pick the
spelling this brief gives, and if none is given, hail the coordinator with the fork and
continue on the other work.

## Reaching the coordinator
`boop beep hail sprefa-coordinator --from <your-lane-name> --body "<one line>"` lands in
the coordinator inbox at its next turn. Use it when blocked, when done (PR number + every
gate number), when this brief is wrong, when you find a defect outside your ownership.
`boop beep lane list` shows your lane name. A lane that ends its turn parks idle; hail
before you stop.

