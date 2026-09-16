# Brief: land PR #408 (sh, bind, host collapse) onto today's main

FIRST ACTIONS, in order, stop and report on any failure:
1. `git fetch origin feature/arrivals-and-ticks && git reset --hard origin/feature/arrivals-and-ticks`
   (your worktree was cut from main; the branch is 23 commits of finished work, head da0d31a3b).
2. `bash v6/tools/doctor-deps.sh` must print DEPS OK.
3. `git merge origin/main`. It conflicts in exactly these six files; resolve as stated:
   - `v6/sprefa-engine-rs/src/hosts.rs`: keep the branch's slash roster and `executor_for`
     arms, ADD main's new names as slash paths: `gh_pulls` -> `/gh/pulls`,
     `dl_tick_cost` -> `/dl/tick_cost`, `files_at` -> `/soopy/files_at`, the ghcache
     additions (grep main's hosts.rs for `graphql`, `gh_pr_batch`) -> `/gh/pr_batch`;
     `LINKED_EXECUTORS` lists every one; the roster test `executor_roster_matches_registry`
     must pass, so add the matching `arrival_executor/2` rows in `registry.pl`.
   - `v6/sprefa-engine-rs/src/executors/fetch.rs` and `tests/executors.rs`: main's version
     (PR #410: headers as columns, follow_link_next, batching test) is the base; re-apply
     only the branch's renames on top.
   - `v6/prolog/conformance/rulings.pl`: keep BOTH sides' rows, append order.
   - `v6/dl/prwatch/prwatch.dl6`: main's body (four `repo(...)` seeds, conditional poll with
     `prev_etag` from #410) in the branch's rel form; `prwatch.adapters.json` stays deleted.
4. Re-spell every program main added since your branch point to the rel form:
   `v6/dl/ghcache/ghcache.dl6` (84 rels; `sh`/`bind` lines become `rel /x/y(...) -> (...)`
   with `key(...)` on the identifying columns per `registry.pl` rows; delete
   `ghcache.adapters.json` once every host is named in the rel), `v6/dl/prwatch/prwatch.dl6`.
   `grep -rlE '^(sh|bind) ' v6/dl v6/prolog/conformance/fixtures` must print nothing.
5. Commit every green step. Push with `--force-with-lease`. Confirm
   `gh pr view 408 --json mergeable` says MERGEABLE.

## Gate, pasted into a PR comment on #408, three runs each for the first two
cd v6/prolog/conformance && timeout 600 swipl -g go -t halt go.pl | grep -c '^PASS'   # 440 on the branch
cd v6/sprefa-engine-rs && timeout 900 cargo test -q                                    # main is 175/0
cd v6 && timeout 600 just plunit                                                        # 1041 or 1042
timeout 600 bash v6/sprefa-engine-rs/grade.sh                                           # graded=440 byte-clean=335
cd v6 && just oracle-rustc && just oracle-knip && just ghcacher-rust && just feature-reach && just crosswalk-gate && just v5-rails && just selfdoc-check
Known on main: `ghcache.dl6` does not COMPILE yet (clock checker, a sibling lane
`fix/clock-check-offset-algebra` is pinning it off); your job is that it PARSES in the rel
form (`swipl -q -l v6/prolog/compile.pl -g "parse only"`: use the parser entry
`use_resolve.pl:parse_source/5`) and that every other gate is green.

## Ownership
Yours: everything the branch already touched plus `v6/dl/ghcache/ghcache.dl6`,
`v6/dl/ghcache/ghcache.adapters.json`, `v6/dl/prwatch/**`, `hosts.rs`, `executors/fetch.rs`,
`tests/executors.rs`, `registry.pl`, `rulings.pl`. FORBIDDEN: `v6/prolog/3_clock_check.pl`
and its test (the clock lane), `v6/tsv2/**`.

## Style laws
No em dashes. Banned: provenance, substrate, load-bearing, regime, refusal, "ground truth".
tracing only. Comment budget: constraints only. Failure ledger entry for the lane stall.
