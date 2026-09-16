# Brief: the resident ghcache run dies in its first batch with "drain overflow"; find the rel that loops, cut it, prove open -> merged live

Base sha: 93b7865bf4c077774c563c8f3ed9d6a1598f3010. FIRST ACTIONS: `git merge --ff-only 93b7865bf4c077774c563c8f3ed9d6a1598f3010`, `bash v6/tools/doctor-deps.sh`
(DEPS OK x2). Never spawn subagents. Commit every green step. PR against `main`.
`export CARGO_BUILD_JOBS=3 RUST_TEST_THREADS=4`; `timeout` on everything; nothing in the
foreground over 10s (background + poll).

## The defect (measured 2026-08-22, three runs)
`GITHUB_TOKEN=$(gh auth token) v6/sprefa-engine-rs/target/release/dl6 run v6/dl/ghcache/ghcache.dl6`
exits within seconds: `Error: drain overflow: ghcache exceeded 100 host/drain ticks in one
batch` (`v6/sprefa-engine-rs/src/run.rs` `LiveLoop::fold`, cap `DRAIN_CAP = 100` at
run.rs:157). One earlier run lived 10 minutes and showed the same loop: `ghcache_call_log`
grew 7 rows per drain tick for 90 ticks with `rate_remaining` flat (no wire traffic;
`ghcache_tick_cost` counted 8 requests per bucket). `fold` drops its accumulated delta
lines on bail, so nothing names the looping rel.

## Facts
- A drain tick = one `drive_tick` + `runner.collect(&deltas)`; collect answers NEW host
  demand rows (witness digest = the identity inputs). Responses are pushed back as the
  next batch; the loop continues while responses are non-empty.
- `ghcache.dl6`: `page_fetch` (`:404-407`) holds while `due` holds (same bucket);
  the `http.get` demand identity includes `headers` (json text carrying
  `If-None-Match` from `poll_state_etag`) and `prev_etag`. A 200 rewrites
  `poll_state_etag` (`<+`), so the demand identity changes and collect fires again;
  the 304 answer writes `poll_state_polled` and re-derives `page_arrival`, whose edge
  consumers (`call_log <+`, `rate_state <+`) fire again. `endpoint_period`
  (`:371`) is `max(period_candidate)` and one candidate depends on `rate_warm`
  (`:362-368`), which depends on `rate_state`, which `due` reads through
  `over_budget`: a cycle through the transport with nothing that converges it.
- The first poll of a cold start is UNAUTHENTICATED: `api_token` is a `<+` fold one
  tick after `token_seen`, so tick 0's requests carry no Authorization and read the
  60/hr bucket (`rate_remaining` 51..58), which puts `rate_warm` on (warn threshold
  500) for the whole run. Fix: `due` (or `page_fetch`) requires `api_token(_)`.

## Build this
1. Instrument first: on overflow, `fold` logs the last ~6 drain ticks' delta lines
   through `tracing::warn!(target: "sprefa_engine_rs::drain", ...)` (no eprintln), then
   bails. Run live once with `RUST_LOG=sprefa_engine_rs::drain=warn`, paste the per-rel
   +/- counts in the PR body. That table is the diagnosis; do not guess past it.
2. Cut the cycle in the program: the demand for one (endpoint, bucket) must be asked
   ONCE per bucket regardless of what the answer writes. Likely shape: `page_fetch` is
   keyed on (endpoint_path, page_url, bucket) through a `<+` so it cannot re-derive
   within the bucket, and `poll_state_polled(EndpointPath, Bucket)` guards `due`
   (`not(poll_state_polled(EndpointPath, Bucket))`). `call_log` must gain exactly one
   row per wire request (COUNT test: a bucket with 8 endpoints = 8 rows).
3. `due` requires `api_token(_)`: tick 0 makes no request; the first poll is
   authenticated (receipt: `min(rate_remaining)` over the run > 4000).
4. Engine rail: the overflow error names the three rels with the most +/- lines in the
   last drain ticks.
5. Live proof, in the PR body: start the run; `gh pr create` a trivial PR from your
   branch (a README line) ; within 2 buckets `ghcache_pull_request` lists it `open`;
   `gh pr merge` it; within 2 buckets `ghcache_pr_transition` has the row
   open -> merged; kill; restart; first poll is 304s; the run stays alive for 10
   buckets with `ghcache_tick_cost` wall_ms under 10000 each. Queries:
   `sqlite3 ~/.agent/dl6.db "select p.number, s.content from ghcache_pull_request p join __str s on s.__id=p.state"`
   and the same join for `ghcache_pr_transition` (from_state, to_state).
   `GITHUB_TOKEN` must be exported for the run. Config: `~/.config/ghcache/config.toml`
   (org hafley66, 4 repos, sync_prs = 1).

## Gate
conformance 440/0, plunit 1059/0, grade.sh graded=440 byte-clean=336, cargo test 158/0,
`bash v6/dl/ghcache/gate.sh` ticks=10 with the same receipt line, `just ghcacher-rust`
goldens=6, ARCH.pl. Read `.github/CI-KNOWN-RED.md` before calling a leg broken.

## Ownership
`v6/dl/ghcache/**`, `v6/sprefa-engine-rs/src/run.rs` (fold/overflow only),
`v6/sprefa-engine-rs/tests/dl6_run.rs`, `docs/failure-modes.md` (append). FORBIDDEN:
`v6/prolog/**` (another lane owns analyze/lower today: file an issue instead),
`v6/tsv2/**`, every other `.dl6`.

## Style laws
No em dashes. Banned in prose and identifiers: provenance, substrate, load-bearing, regime,
refusal, "ground truth" (say oracle). Comments state constraints only. Descriptive dl
variable names. One ledger entry: "a demand whose identity its own answer rewrites".

## Reaching the coordinator
`boop beep hail sprefa-coordinator --from <your-lane-name> --body "<one line>"`. When done:
PR number, the per-rel loop table, every gate number, the open -> merged receipt.
A lane that ends its turn parks idle; hail first.
