# Brief (v2): ghcache.dl6 compiles, folds a simulated schedule, then runs live against the org

Base sha: the spawner prints it. FIRST ACTIONS: `git fetch origin feature/ghcache-compiles && git reset --hard origin/feature/ghcache-compiles` (two commits of the previous lane: the aggregate fix and live_cadence), then `git merge origin/main`; `bash
v6/tools/doctor-deps.sh` (DEPS OK). Never spawn subagents. Commit every green step. PR
against `main`. `timeout` on every command; `export CARGO_BUILD_JOBS=3 RUST_TEST_THREADS=4`.

## Where it stands (coordinator, 2026-08-21, on main at your base sha)
The clock path walk is pinned off (`3_clock_check.pl` `clock_path_walk_enabled :- fail.`,
ruling `clock_path_check_pinned_off`), so `v6/dl/ghcache/ghcache.dl6` reaches lowering in
1.6s. The coordinator fixed four lowering stops by hand and the fifth is yours:
- `trigger_arg_not_var(200|60|404)`: a literal in an arrival goal of an edge rule (`<+`)
  is a stop (`lower.pl:4184`); fixed by a fresh variable plus `Var == literal,` after the
  goal (11 sites `RespStatus == 200`, `Lit`, `Period == 60`).
- `edge_into_unkeyed_set(not_an_org/1)`: `lower.pl:4006`; fixed with `key(1)`.
- DONE by the previous lane (commits on the branch): `aggregate_group_not_delta_local` and the literal interval reads (`live_cadence`). The previous lane reported "conformance 439 clean, dl6 built, live 2-bucket poll running" and then ended its turn, so nothing was pasted and `ghcache_call_log` in ~/.agent/dl6.db holds 0 rows. Redo the receipts.
- NEXT if compile still stops: `aggregate_group_not_delta_local(rate_pool(_,_,_,count(_)))`. Find the throw site
  (`grep -rn aggregate_group_not_delta_local v6/prolog`), read what "delta local" means
  there, rewrite `rate_pool` so the aggregate groups on columns the delta carries, and
  keep going until `swipl -q -l v6/prolog/compile.pl -l v6/prolog/emit_rust.pl -g
  "catch(compile_dl6('v6/dl/ghcache/ghcache.dl6','/tmp/ghcache.rs',
  [emitter(emit_rust:emit_program)]),E,(print(E),nl))" -t halt` prints nothing and
  `/tmp/ghcache.rs` exists. Each stop you hit: name the throw site in the PR, state whether
  the program or the compiler was wrong, fix the program unless the compiler is plainly
  wrong (then file an issue, do not touch `v6/prolog/**`).
Every program edit must keep `v6/dl/ghcache/README.md`'s ability map true; update rows
you change.

## Then, three receipts
1. SIMULATED: `v6/dl/ghcache/ghcache.schedule.json` (exists) through the Rust door with
   `/http/fetch` and `/gh/pr_batch` answered by canned rows (`--arrive` batches or the
   `fixture` executor, whichever `v6/dl/ghcacher/gate.sh` uses today). Then:
   `sqlite3 ~/.agent/dl6.db "select status, count(*) from ghcache_call_log group by status"`
   and `"select bucket, count(*) from ghcache_poll group by bucket"`. Paste both. A poll
   that is not due must produce NO request row: prove with a schedule where the rate
   budget is below `rate_stop_threshold` for three buckets (0 requests in that window).
2. LIVE: `dl6 run v6/dl/ghcache/ghcache.dl6` against `~/.config/ghcache/config.toml`
   (org `hafley66`, repos instant/sprefa/hafley-rs/hafley-rxjs) for two buckets, then kill.
   Paste the same two queries; pass 2 must be 304s with `bytes = 0` for every unchanged
   endpoint. Paste `rate_remaining` before and after; the drop must be <= the number of
   distinct endpoints polled once.
3. `v_tick_cost` (or `ghcache_tick_cost`) for those ticks: wall_ms and rss_kb per bucket,
   every tick under 10s.

## Gate
conformance (`cd v6/prolog/conformance && swipl -g go -t halt go.pl`) count unchanged or
grown; `just plunit`; `bash v6/sprefa-engine-rs/grade.sh` graded/byte-clean unchanged;
`just ghcacher-rust` goldens=6.

## Ownership
Yours: `v6/dl/ghcache/**`, `v6/dl/ghcacher/gate.sh` read only. FORBIDDEN: `v6/prolog/**`
(file issues), `src/**` except a one-line adapter row in `hosts.rs` if a name is missing,
`v6/dl/prwatch/**` (another lane re-spells it), `v6/tsv2/**`.

## Style laws
No em dashes. Banned: provenance, substrate, load-bearing, regime, refusal, "ground truth".
Comment budget: constraints only. Failure ledger entry: "a lane that never ran its program".

## Reaching the coordinator
`boop beep hail sprefa-coordinator --from <your-lane-name> --body "<one line>"` lands in the
coordinator hook inbox at its next turn. Use it when blocked, when done (PR number + gate
numbers), when this brief is wrong, or when you find a defect outside your ownership.

## Turn law (boop treats the end of your turn as your death)
Never end your turn before the PR is posted and its gate numbers are in a `boop beep hail
sprefa-coordinator` body. Waiting on a background job: poll it with an `until` loop inside
one Bash call, never by ending the turn. A finding that changes scope: hail it in one line
AND keep working on the parts it does not change.
