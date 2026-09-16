# Brief: PRs watched live, state tracked over time, by a dl6 program resident in the background

Base sha: the spawner prints it. FIRST ACTION `git merge --ff-only <sha>`; failure = stop and
report. Never spawn subagents. PR against `main`. `export CARGO_BUILD_JOBS=3
RUST_TEST_THREADS=4`; `timeout` on every command.

## The user's ask (2026-08-21, verbatim)
"i want pr's live watched and their state tracked over time from polling and for you to prove
the subagents did the jobs with pr database info from the program running efficiently in the
background", "measuring gh and itself over time".

## Inherit, do not restart
1. `git fetch origin feature/dl6-run-watch wip/dl6-run-watch-salvage`. PR #405 (one commit
   `790dea415`) is the lane's committed half; `wip/dl6-run-watch-salvage` is the SAME lane's
   later uncommitted work (`run.rs` 848 lines, `runtime.rs` 682, `executors/clock.rs`,
   `executors/watch.rs`) that the coordinator saved after the lane died. Reconcile the two
   onto `main` (the Cargo.toml/lock, executors/mod.rs, hosts.rs, lib.rs conflicts are
   additive: keep both sides). Close PR #405 in favour of yours.
2. The verb is ONE: `dl6 run prog.dl6`. The file says whether it stays resident (any rel
   routed to a continuing executor). Client hashes source, transmits TEXT over the UDS
   socket, runtime compiles once per hash (`runtime.rs:2-14`). `--db <file>` leaves a
   cold-queryable SQLite with `v_<query>` views and `__meta`. `--fail-on <query>`.

## Build
- `gh.pulls` executor (new file `src/executors/pulls.rs`, ureq via the agent in
  `executors/fetch.rs:18`, conditional GET with ETag, bearer from `fetch.rs:63`):
  input `repo_slug`, `state`; output one row per PR: number, title, head_ref, head_sha,
  state, merged_at, updated_at, mergeable, review_decision. Paginate. One call per
  (repo, bucket); 304 = 0 bytes and the same rows.
- `v6/dl/prwatch/prwatch.dl6`: `clock.tick(every: 60)` drives `poll`; `pr_event(...) log
  keep(all)` keeps every observed state per tick; `pr_state(...) key(1)` is current;
  `pr_transition(number, from, to, at_bucket)` is derived; `lane_proof(branch, pr, merged_sha)`
  joins PRs whose head_ref starts with `feature/|fix/|plan/` to MERGED. `? lane_proof` and
  `? pr_transition` are the outputs.
- Self-measurement in the same db: a `tick_cost(bucket, executor, wall_ms, bytes, rss_kb)`
  rel fed by the runtime from `DL_TRACE_SUMMARY` spans (`src/trace.rs`). `? tick_cost` is an
  output. RSS flat across 30 ticks is a test with numbers.
- Run it: `dl6 run v6/dl/prwatch/prwatch.dl6 --db ~/.agent/prwatch.db` in the background
  against `hafley66/sprefa`, let it tick for 10 minutes, and paste `sqlite3
  ~/.agent/prwatch.db 'select * from v_lane_proof'` and `v_tick_cost` in the PR. That paste is
  the receipt the user asked for.

## Ownership (disjoint)
Yours: `src/run.rs`, `src/runtime.rs`, `src/executors/{clock,watch,pulls}.rs`, `src/bin/dl6.rs`,
`src/trace.rs`, `v6/dl/prwatch/**`, `src/serve.rs`. In `executors/mod.rs` and `hosts.rs` ONLY
the lines that register your three names. FORBIDDEN: `v6/prolog/**` (another lane is
collapsing bind/sh/host into one rel form; spell your program with the form the conformance
fixtures on main use today and note in the PR that the other lane will rewrite it),
`src/change_facts.rs`, `src/executors/{repo_at,git_*,dep_crawl,fetch}.rs`, `v6/tsv2/**`.

## Gate
cd v6/sprefa-engine-rs && timeout 600 cargo test -q   # 144/0 today, yours added
timeout 600 bash v6/sprefa-engine-rs/grade.sh          # graded=439 byte-clean=335
cd v6 && just ghcacher-rust                            # GHCACHER_RUST_DOOR_HOLDS goldens=6
The 10-minute background run: every tick under 10s, RSS series pasted.

## Style laws
No em dashes; banned: provenance, substrate, load-bearing, regime, refusal, "ground truth".
tracing only, no eprintln in src. Comment budget: constraints only. Failure ledger entry for
the lane death that lost 1718 lines uncommitted (commit every green step).
