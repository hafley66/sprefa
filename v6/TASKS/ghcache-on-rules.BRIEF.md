# Brief: the GitHub cacher and PR polling live in dl6 rules over ONE transport rel

Base sha: the spawner prints it (main 1e39b557874ff790b2a2d8d0591e134216eda558).

## The user's decision (2026-08-22)
"I want a general HTTP relation we host; the side effect is new rows arrive. I need HTTP
most of the time, then files and git, and we add as we go. Get us back to the GitHub
cacher and efficient PR polling ALL inside dl6." Today's executors ported ghcacher's
Rust logic into the engine instead; this arc moves it back into the program.

## What is wrong today (verified, cite in the PR)
`v6/sprefa-engine-rs/src/executors/`:
- `fetch.rs:31-50` process-private ETag+body HashMap; `:255-260` 304 body
  substitution; `:63-68` token lookup; `:192-234` Link rel=next walk capped at 10.
  The program already carries the ETag relationally (`v6/dl/ghcache/ghcache.dl6:308`
  `poll_state_etag`, `:340`), so the HashMap duplicates it and loses it on restart
  (issue `issues/dl6-run-restart-loses-etags`: every restart re-downloads ~1 MB).
- `graphql.rs` (459 lines): batch cap 20 `:26-65`, query building `:115-131`,
  fan-out into 6 arrays `:194-269`, `pr_state` `:302-308`, status merge `:310-358`.
- `pulls.rs` (192): pagination cap 5 `:61-93`, state vocabulary `:135-140`,
  `review_decision` `:176-192`. `repos.rs` (150): pagination `:53-82`.
- `cost.rs` atomics `:21-55` are the engine measuring itself; leave them.
- `issues/ghcache-dl6-poll`: periods are config SECONDS compared against
  minute-quantized `current_clock(60, Bucket)` buckets, so a 60s poll fires hourly.
- `issues/ghcache-gh-username-unsourced`: `gh_username` (`ghcache.dl6:204`) has no
  rule, so the user-events endpoint never polls.
- `issues/host-collect-*`: `hosts.rs:1620` `collect` runs demands sequentially; 8
  GETs took 26s on the first poll bucket (10-second law).

## Build this
1. ONE transport executor, spelled in programs at this base as
   `rel http.get(url: key(text), headers: json, prev_etag: text, bucket: int)
   -> (status: int, headers: json, body: json, bytes: int).`
   (the `use http.` + bare `get` spelling lands from another lane; the coordinator
   re-spells your files after both merge, so write `http.get` and do not build any
   parser change). Rust: `src/executors/http.rs` `HttpGetExecutor`: build the request
   from the row (every header comes from the `headers` json column, including
   `Authorization` and `If-None-Match` when the program supplies them), send it with
   the existing `ureq` agent, answer status + every response header as a json object +
   body (json when parseable, else a json string) + bytes. NO cache, NO pagination, NO
   token lookup, NO 304 substitution, no static state beyond the connection pool. A
   `rel http.post(url: key(text), headers: json, body: json, bucket: int) -> (...)` with
   the same output shape, for GraphQL. Roster: add both to `hosts.rs` `LINKED_EXECUTORS`
   and `executor_for`, and to `registry.pl`'s roster (one row each; the roster test
   pins the two equal). Concurrency: `hosts.rs:1620` `collect` dispatches the
   `http.*` demands of one tick on a bounded pool (`std::thread::scope`, cap from
   `apply_daemon_budget`), joins in demand order; receipt: 8 endpoints against a 3s
   stub listener answer under 4s (test with a local listener; `DL_GITHUB_API_BASE`
   already exists at `fetch.rs:54-61`, keep that env door).
2. `v6/dl/ghcache/ghcache.dl6` carries everything else as rules, in the one db:
   - token: `rel env.var(name: key(text)) -> (value: text)` already exists; the program
     reads `GITHUB_TOKEN` then `GH_TOKEN` and builds the `headers` json (`Accept`,
     `Authorization`, `If-None-Match` from `poll_state_etag`).
   - 304: `last_body(endpoint_path: key(text), body: json) <+ response status 200`;
     a 304 response joins `last_body`; `bytes` stays what the wire moved.
   - pagination: `next_page(endpoint_path, next_url)` decoded from the `link` header
     (RFC 5988 `rel="next"`), one demand row per page in the same bucket, pages
     unioned by rule; the cap is a program fact `page_cap(10)`.
   - rate headers `x-ratelimit-remaining`, `x-ratelimit-reset`, `x-poll-interval`
     decoded from `headers` by `decode`.
   - GraphQL PR batch: the query text is built by rule (`concat`) from the repo set,
     `http.post` carries it, the 6 fan-outs (pulls, reviews, comments, labels,
     requested_reviewers, status_checks), `pr_state` and `review_decision` are
     `decode` rules over `body`. Batch size is a fact (`batch_size(20)` exists at
     `:559`).
   - fix `issues/ghcache-dl6-poll`: a `clock_granularity(60)` fact, periods as
     `ceil(seconds / 60)` buckets; COUNT test: 60s period over 3 buckets = 3 polls.
   - fix `gh_username`: one `http.get` of `user`, keyed fold of `login`.
   - `v6/dl/prwatch/prwatch.dl6` (111 lines) becomes rules inside ghcache.dl6
     (pull_request sync over the PR batch, `pr_state` over time, lane_proof view) and
     the file is deleted with its README folded into `v6/dl/ghcache/README.md`.
   - restart: every keyed fold the poll reads (`poll_state_*`, `last_body`,
     `rate_state`) must survive `dl6 run` restart. Find why `run.rs` (`:287`
     `reset_program_objects`) empties them, make keyed program rels persist across
     starts in `~/.agent/dl6.db` (the one-db law, CLAUDE.md 2026-08-21), and prove it:
     start, poll (200 x N), kill, start, first poll is 304 x N with bytes=0.
3. Delete: `fetch.rs` HashMap + `cached_entry/remember/forget_all` + `follow_link_next`
   + `bearer_token` (keep `absolute_url` and the agent if `http.rs` uses them, else
   delete the file), `pulls.rs`, `repos.rs`, `graphql.rs`, `GhPrBatchExecutor`, the
   `/http/fetch`, `/gh/*` roster rows, their tests in `tests/executors.rs` and
   `tests/live_hosts.rs` (replace with `http.get`/`http.post` tests against a local
   listener). The ghcacher goldens (`v6/dl/ghcacher/*.dl6`, `gate.sh`, scripted
   schedules, `v6/dl/fixtures/ghcacher*.dl6`, `v6/dl/fixtures/crawl_org.dl6`) move onto
   `http.get`; `just ghcacher-rust` keeps goldens=6 (regenerate the scripted responses
   to the new row shape, keep the tick logs semantically equal, explain every diff in
   the PR).
4. Receipts in the PR body: (a) simulated schedule through `gate.sh`; (b) live
   `GITHUB_TOKEN=$(gh auth token) dl6 run v6/dl/ghcache/ghcache.dl6` against
   `~/.config/ghcache/config.toml` (org hafley66; instant, sprefa, hafley-rs,
   hafley-rxjs) for 3 buckets: `ghcache_call_log` status counts per bucket, pass 2+
   all 304 bytes=0, `rate_remaining` drop <= distinct endpoints; (c) kill + restart,
   first poll 304s; (d) `ghcache_tick_cost` every bucket wall_ms under 10000;
   (e) the `ghcache_pull_request` view lists open PRs of the org with state. Set
   `~/.config/ghcache/config.toml` `poll_interval_seconds` back to 60 when done (it
   reads 1 now).

## Ownership
Yours: `v6/sprefa-engine-rs/src/executors/{fetch,http,pulls,repos,graphql}.rs`,
`src/executors/mod.rs` exports for those, `src/hosts.rs` (only the roster rows and
`collect`), `src/run.rs` persistence, `tests/executors.rs`, `tests/live_hosts.rs`,
`v6/dl/ghcache/**`, `v6/dl/prwatch/**`, `v6/dl/ghcacher/**`,
`v6/dl/fixtures/ghcacher*.dl6`, `v6/dl/fixtures/crawl_org.dl6`, `registry.pl` roster
rows ONLY, `docs/failure-modes.md` (append). FORBIDDEN: every other `v6/prolog/**`
file (another lane owns the parser and rulings; put your contract text in
`v6/dl/ghcache/README.md` and the coordinator adds the ruling row),
`src/executors/{dep_crawl,git_refs,git_history,repo_at}.rs` (another lane), every
other `.dl6`, `v6/tsv2/**`.

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

