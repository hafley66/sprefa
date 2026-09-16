# engine-tick-trace

Issue: `issuectl show engine-tick-trace` (read it first, it carries every line number).
Base: `git merge --ff-only <sha the coordinator states>` is your first action. Fail = stop and hail.
Branch: `fix/engine-tick-trace`. PR to main when every gate below is green.

## You own (nobody else touches these this week)
- `v6/sprefa-engine-rs/src/run.rs` (ONE edit: `force_summary()` beside `arm()` at :694)
- `v6/sprefa-engine-rs/src/trace.rs` (doc comment for the two new verbs only)
- `v6/sprefa-engine-rs/src/executors/cost.rs`
- `v6/sprefa-engine-rs/src/ordered.rs` (ONLY `Scope::verb`/`Scope::phase` wrappers; do not move, reorder or delete a statement; a second lane owns its logic)
- `v6/sprefa-engine-rs/tests/tick_trace.rs` (new)
- `v6/dl/ghcache/ghcache.dl6`, `v6/dl/ghcache/gate.sh`, `v6/dl/ghcache/ghcache.schedule.json` (item 3 only)
- `docs/failure-modes.md` (append one entry, next free number)

Forbidden: `incremental.rs`, `program.rs`, `sql.rs`, anything under `v6/prolog`, `v6/tsv2`.

## Items
1. Arm the live trace. `run.rs:694` currently `crate::trace::arm();` add `crate::trace::force_summary();` on the line before it. Receipt: run `dl6 run v6/dl/ghcache/ghcache.dl6` for two minute buckets (background, log to a file, poll with `timeout 10`), then `sqlite3 ~/.agent/dl6.db "select bucket,executor,wall_ms,sqlite_ms,calls from __txt_ghcache_engine_tick_cost order by bucket desc limit 5"` shows `sqlite_ms > 0` and more than the single `wall` executor row. Paste the rows in the PR.
2. Label the ordered path. In `ordered.rs` wrap with `let _scope = crate::trace::Scope::verb(<verb>, <rel>, "ordered");`:
   - `read_snapshot` per-rel loop body (`:33`) -> verb `"snapshot"`
   - `recompute_levels` per-level body (`:226-228`, `:244-248`, `:260-263`) -> verb `"recompute"`, rel = `statement.head_rel`
   - `apply_occurrence` -> verb `"edge_write"`, rel = the occurrence's rel
   - `apply_retention` per rel -> `"clear"`
   - the `stage_ordered_frontiers` / `stage_departures` calls are already scoped inside `incremental.rs`; verify with the table, do not add a second scope.
   `Scope::verb` takes `&'static str` for the verb; the verb list lives in the `trace.rs` doc comment at the `phase` fn ("the six verbs"); add `snapshot` and `recompute` there.
   Receipt: `cd v6 && DL_ADAPTERS_DIR=$PWD/dl/ghcache DL_TRACE_SUMMARY=1 sprefa-engine-rs/target/release/emit_rust_harness <compiled>.rs dl/ghcache/ghcache.schedule.json --final 2>&1 | grep -A40 'DL_TRACE_SUMMARY =='` shows `unlabelled` with 0 calls. Compile the program with the exact swipl line in `v6/dl/ghcache/gate.sh`.
3. Open -> merged is never recorded (PR #422 "Not delivered"). `ghcache.dl6:823-838` `pr_selection` asks `states: OPEN`. Add a SECOND selection per repo, alias `<RepoAlias>_recent`, `pullRequests(first: 10, states: [MERGED, CLOSED], orderBy: {field: UPDATED_AT, direction: DESC})`, same field list. Measured 2026-08-23: 141 KB, 2.3-4.5 s for 4 repos (the unfiltered `first: 100` form is 755 KB, 7-10 s and 502s against the 10 s `REQUEST_TIMEOUT` at `executors/http.rs:29`; do not use it). `gql_pull` and the other `gql_*` decodes at `:918-1000` key on `$RepoAlias`; `pr_batch_alias` (`:820`) must map the `_recent` alias to the same `(Owner, Name)`. `pr_transition` (`:1149`) then fires on the state change. Extend `ghcache.schedule.json` with a scripted answer where a PR is OPEN in bucket N and appears in the `_recent` alias as MERGED in bucket N+1; `gate.sh` asserts one `pr_transition` row `open -> merged`. Live receipt: open a probe PR on `hafley66/sprefa` (branch `probe/pr-transition`, one whitespace file, body "probe, close me"), merge it, show the `ghcache_pr_transition` row from `~/.agent/dl6.db`. Delete the probe branch after.
4. `tests/tick_trace.rs`: fold the ghcache schedule with `trace::force_summary()` (the emitted program is built by the existing `emit_rust_harness` path; copy how `tests/dl6_run.rs` drives a program), assert `unlabelled` calls == 0 and `recompute` calls == `levels.len() * 2 * ticks`. The second number pins today's defect; the `ordered-tick-recompute` lane will lower it.
5. Ledger entry in `docs/failure-modes.md`: incident (the zero rows), RCA, fail-pre-fix (item 4 red at base), rail (item 4), entry (the numbers).

## Style laws (CLAUDE.md, enforced)
No `eprintln!`; `tracing` only. Comments state constraints the code cannot show; no change-log narrative. No em dashes. Banned words in prose and identifiers: provenance, substrate, load-bearing, regime, ground truth (say oracle), refusal. dl variable names descriptive. Every `.dl6` change keeps `emit_ts.pl` output byte-identical for unchanged programs (tsv2 is paused; do not run the sweep).

## Gates, all green before the PR, numbers in the PR body
```
cd v6/prolog/conformance && swipl -g go -t halt go.pl      # 444/0
cd v6 && just plunit                                        # 1065/0
bash v6/sprefa-engine-rs/grade.sh                           # graded=444 byte-clean=340
cd v6/sprefa-engine-rs && cargo test --workspace            # 161/0 + yours
bash v6/dl/ghcache/gate.sh                                  # ticks=11 today; your schedule adds buckets, state the new number
cd v6 && just ghcacher-rust                                 # goldens=6
cd v6/prolog && swipl -g go -t halt ARCH.pl                 # 7/0
```
Run batteries in the background with a per-command cap (`timeout`), never foreground-wait more than 10 s on one command. Commit after each item. `git status` clean before you hail.

Done: `boop beep hail sprefa-coordinator --from engine-tick-trace --body "PR #<n>: <gate numbers>, <sqlite_ms rows>, pr_transition row <yes/no>"`.
Blocked or brief wrong: hail the same way, one line, and stop.
