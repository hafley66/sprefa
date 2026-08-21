# Brief: one rel, external arrivals, ticks. `sh`, `bind`, `host` die.

Base sha: the spawner prints it. FIRST ACTION `git merge --ff-only <sha>`; failure = stop and
report. Never spawn subagents. One PR against `main`. `export CARGO_BUILD_JOBS=3
RUST_TEST_THREADS=4`; wrap every command in `timeout`; no single operation over 10s except
cargo build and scip indexing.

## The user's decision (2026-08-21, verbatim, this is the design, do not re-litigate)
"all the old bind and sh and host shit is now just arrival and ticks", "send rel's in and out
of the thing", "non confusing named and well namespaced". `bind` is DEAD (interval and watch
forms included). No new syntax beyond what the plan names.

## Start from the plan, then build it
1. `git fetch origin plan/one-rel-with-arrivals` and `git merge` that branch first (PR #401:
   `plans/2026-08-21-one-rel-with-arrivals.PLAN.md` + `.visual.human.unga.md` + the probe
   fixture). Its section 6 names the one blocker and section 8 the three steps. Those steps
   are your task list. Where the plan leaves a fork open ("9. Four questions"), pick the
   reading that is most consistent with "a rel whose rows arrive from outside is still just a
   rel", write the pick into `v6/prolog/conformance/rulings.pl` as a row citing this brief,
   and keep going. Do not wait.
2. Every construct that reaches an executor is a `rel` declaration plus a namespaced executor
   name. Namespace rule: `<executor>.<question>` dotted, as `scip.diet.call` already is
   (`v6/prolog/compile/registry.pl:498-505`). No bare `files`, no `fetch`; `soopy.files`,
   `http.fetch`, `gh.repos`, `gh.pulls`, `clock.tick`, `soopy.watch`. The registry is the one
   roster; `LINKED_EXECUTORS` in `v6/sprefa-engine-rs/src/hosts.rs` must list the same
   names, and a test asserts the two rosters are equal.
3. Ticks: an external arrival batch is one tick, exactly as today's `--arrive` path
   (`src/bin/emit_rust_harness.rs`). A continuing executor (`ExecutorCadence::Continuing` in
   the salvage branch `wip/dl6-run-watch-salvage`, `executors/clock.rs`, `executors/watch.rs`)
   re-answers and that re-answer is a tick. Nothing else is a tick.
4. Lists as the batching seam: the user asked "we have lists in this language so can we not
   figure out collect or batching in the lang itself". Today `HttpFetchExecutor::run`
   (`executors/fetch.rs:127`) answers ONE url per call, so N endpoints = N calls per tick.
   Find whether an input column typed `list(text)` (see `registry.pl:294` `split/2 ->
   list(text)` and `0_generic_expand.pl` `list_flavor_artifacts/2`) can reach an executor as
   one demand. If yes, build it and prove with a COUNT test: 6 endpoints, 1 executor call. If
   no, the PLAN gains a section with the throw site and the user decides.

## Ownership (disjoint from the two lanes running beside you)
Yours: `v6/prolog/**`, `v6/prolog/conformance/**`, `plans/**`, `v6/sprefa-engine-rs/src/hosts.rs`
(roster + names only), `v6/sprefa-engine-rs/src/executors/fetch.rs`, every `*.adapters.json`
and `*.dl6` under `v6/dl/**` that spells `bind`/`sh`/`host` (rewrite them to the one form).
FORBIDDEN: `src/run.rs`, `src/runtime.rs`, `src/executors/{clock,watch,pulls}.rs`,
`src/change_facts.rs`, `src/executors/{repo_at,git_refs,git_history,dep_crawl}.rs`,
`v6/dl/crosswalk/fixtures/**`, `v6/tsv2/**` (paused; `emit_ts.pl` output for unchanged
programs stays byte-identical, prove with `git diff --stat` on `compile/out/`).

## Gate (run each three times, paste the numbers)
cd v6/prolog/conformance && timeout 600 swipl -g go -t halt go.pl     # 439 PASS today
cd v6 && timeout 600 just plunit                                        # 1041/0 today
timeout 600 bash v6/sprefa-engine-rs/grade.sh                           # graded=439 byte-clean=335
cd v6/sprefa-engine-rs && timeout 600 cargo test -q                     # 144/0 today
cd v6 && just oracle-rustc && just oracle-knip && just ghcacher-rust && just feature-reach && just crosswalk-gate && just v5-rails
Every fixture that used `bind`/`sh`/`host` still passes under the one form. Conformance count
may only grow.

## Style laws
No em dashes. Banned words: provenance, substrate, load-bearing, regime, refusal, "ground
truth" (say oracle). Comment budget: constraints only. dl variable names descriptive. Every
new class declares its interface in the package's types header. No eprintln in src.
Failure ledger: `docs/failure-modes.md`, next number after the last (the file has TWO `## 60`
headers at :2192 and :2259; renumber the whole tail in your PR, one pass).
