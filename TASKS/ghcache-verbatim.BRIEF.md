# Brief: ghcache, verbatim, in dl6

Base sha: the spawner prints it. FIRST ACTION `git merge --ff-only <sha>`; failure = stop and
report. Never spawn subagents. PR against `main`. `export CARGO_BUILD_JOBS=3
RUST_TEST_THREADS=4`; `timeout` on every command.

## The user's ask (2026-08-21, verbatim)
"i have a gh repo that is called ghcacher and its the og of this, i want it in here verbatim
in dl6 tho. i want verbatim abilities bc i dont want my github points dying every time i poll
ineffectively."

The original: `~/projects/ghcacher` (Rust, 3973 lines in `src/`, github `hafley66/ghcacher`).
Read ALL of it before writing a line: `src/schema.sql` (16 tables), `src/gh.rs` (ETag,
`If-None-Match`, `X-Poll-Interval`, `X-RateLimit-Remaining/Reset`, `throttle_if_needed`
at :143-170, GraphQL `inject_rate_limit` at :378), `src/sync/{prs,branches,events,
notifications}.rs`, `src/query/*.rs`, `src/config.rs` (tiers: `poll_interval_seconds`,
`org_repo_discovery_interval_seconds`, `rate_warn_threshold`, `rate_stop_threshold`),
`src/cmd.rs` (watch loop, subscriptions, SSE broadcast of `change_log`), `src/checkout.rs`,
`src/worktree.rs`, `demo.sh` (the acceptance: second pass is all 304s).

## Deliverable: `v6/dl/ghcache/ghcache.dl6` + `v6/dl/ghcache/README.md`
One program, run as `dl6 run v6/dl/ghcache/ghcache.dl6 --db ~/.agent/ghcache.db` on the
resident runtime (PR #407's lane is building `dl6 run`; until it merges, drive it with
`emit_rust_harness` exactly as `v6/dl/ghcacher/gate.sh` does, and say so in the PR).
Every ability of the original, mapped one to one, with a table in the README:
`original file:line -> dl6 rel/rule`. Nothing dropped silently; anything the language cannot
express yet gets a row with the throw site and a filed issue.

The abilities that protect the rate budget are NOT optional:
1. `poll_state(endpoint, etag, last_modified, poll_interval)` keyed per endpoint; every GET
   sends `If-None-Match`; a 304 is zero bytes and zero rows of change.
2. `call_log(at, api_type, endpoint, status, rate_remaining, rate_reset, bytes)` as a
   `log keep(all)` rel fed from the response headers.
3. Throttle as a RULE, not a sleep in Rust: `may_poll(endpoint, bucket)` holds only when the
   latest `call_log.rate_remaining` for that api_type is above `rate_stop_threshold`, and the
   clock bucket respects `max(poll_interval_seconds, X-Poll-Interval)`; below
   `rate_warn_threshold` the bucket stretches. Below stop, no demand is minted until
   `rate_reset`. A COUNT test proves zero GETs are issued in the stopped window.
4. Org discovery on its own slower interval (`org_repo_discovery_interval_seconds`).
5. `change_log` as the derived delta rel (what changed this tick), the thing SSE broadcast.
6. The 16 tables as rels under the storage law: INTEGER surrogate ids, natural keys
   (`owner/name`, PR number) ONCE in a dictionary rel with UNIQUE; no composite TEXT keys.
   Read `.claude/skills/sql-relational-design` and `.claude/skills/sqlite-costs` first.
7. `pull_request`, `pr_review`, `pr_comment`, `pr_status_check`, `pr_label`,
   `pr_requested_reviewer`, `branch`, `repo_event`, `notification`, `checkout`, `worktree`:
   each synced by the same endpoints the original hits, same fields.

## Executor side (Rust, keep it generic)
- `executors/fetch.rs` `HttpFetchExecutor`: expose response headers as output columns
  (`etag`, `last_modified`, `poll_interval`, `rate_remaining`, `rate_reset`, `status`,
  `bytes`) and honour a `follow_link_next` input for pagination. No per-endpoint Rust.
- A JSON-array-to-rows executor if the language has no column-plane JSON element read; name
  it `json.rows(body) -> (index, element)` (the pr-watch lane was asked for the same thing;
  check `origin/feature/pr-watch-resident` first and reuse it if it exists, do not build twice).
- GraphQL: the original injects `rateLimit { remaining resetAt }` into every query
  (`gh.rs:378`). Same, as a `graphql.query(query: key(text)) -> (data: json, rate_remaining,
  rate_reset)` executor.

## Spelling
Use the executor-rel form the conformance fixtures on `main` use at your base sha. If
`feature/arrivals-and-ticks` has merged by then, its form: `rel /http/fetch(url: key(text),
prev_etag: text) -> (...)`. Check `git log origin/main --oneline -20` for "arrivals" first.

## Receipts
- `demo.sh` equivalent: sync `hafley66/sprefa` twice; paste `select status, count(*) from
  call_log group by status` showing pass 2 is all 304.
- `select rate_remaining from call_log order by id desc limit 1` before and after a 10-minute
  watch; the drop must be <= the number of distinct endpoints polled.
- COUNT test for the stopped window (item 3). Engine `cargo test -q` green plus yours.
- RSS flat across the 10-minute watch, series pasted.

## Ownership (disjoint)
Yours: `v6/dl/ghcache/**`, `src/executors/fetch.rs`, `src/executors/graphql.rs` (new),
`src/executors/json_rows.rs` (new, unless reused), tests for those. FORBIDDEN:
`v6/prolog/**`, `src/run.rs`, `src/runtime.rs`, `src/executors/{clock,watch,pulls}.rs`,
`v6/dl/prwatch/**`, `v6/dl/ghcacher/**` (the six goldens stay byte-identical), `v6/tsv2/**`.

## Style laws
No em dashes; banned: provenance, substrate, load-bearing, regime, refusal, "ground truth".
tracing only, no eprintln. Comment budget: constraints only. Every new class declares its
interface. Failure ledger entry for any incident.
