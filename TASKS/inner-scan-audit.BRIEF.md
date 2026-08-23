# inner-scan-audit

Issue: `issuectl show inner-scan-audit` (the run line and the (a)/(b)/(c) classification are there). Base: `git merge --ff-only <sha the coordinator states>` first; fail = stop and hail. Branch `chore/inner-scan-audit`. PR to main.

## Deliverable
1. Run the issue's line, save the log, classify all 215 `scan=true` plans into (a) json_each virtual-table scans, (b) correlated scalar subqueries that SEARCH `__ref_*` by rowid, (c) a real inner SCAN of a base, frontier or temp table after a join. Table in the PR body: class, count, three example shapes each.
2. Bake the rule into `sql.rs::explain_once`: `scan=true` only for (c); add `scan_kind` field with `json_each` / `ref_subquery` / `inner`. Unit test on three literal plan texts.
3. For every (c): the index or join order that fixes it lives in the emitted DDL (`v6/prolog/lower.pl`, grep `CREATE INDEX`) or the level SQL shape; fix ONLY where an additive EXPLAIN test can assert SEARCH afterwards (pattern: `tests/shared_frontier.rs:119`). Each fix = its own commit with the test. A (c) you cannot fix with a SEARCH receipt is listed, not touched.
4. `grade.sh byte-clean=340` and the ghcache tick log must not move: an index changes no row.

## You own
`v6/sprefa-engine-rs/src/sql.rs` (`explain_once` and its test only), `v6/sprefa-engine-rs/tests/explain_*.rs`, `v6/prolog/lower.pl` (index emission sites only; the one-tick-path lane may touch lower.pl for an edgestmt field, so keep your hunks inside the DDL/index predicates and rebase when I hail you its sha), `v6/prolog/compile/test/plunit_tests.pl` (additive tests for your index predicates).
Forbidden: `ordered.rs`, `incremental.rs`, `program.rs`, `run.rs`, `driver.rs`, `emit_rust.pl`, `emit_ts.pl`, `v6/dl/**`.

## Gates, all green before the PR, numbers in the PR body
```
cd v6/prolog/conformance && swipl -g go -t halt go.pl      # 444/0
cd v6 && just plunit                                        # 1076/0
bash v6/sprefa-engine-rs/grade.sh                           # graded=444 byte-clean=340
cd v6/sprefa-engine-rs && cargo test --workspace            # 163/0 + yours
bash v6/dl/ghcache/gate.sh                                  # ticks=14 pr_transition_open_merged=1
cd v6 && just ghcacher-rust                                 # goldens=6
cd v6/prolog && swipl -g go -t halt ARCH.pl                 # 7/0
```
Batteries in the background with `timeout`; never foreground-wait more than 10 s on one command. Commit per item; PUSH before you report; a result with nothing pushed is not a result.

## Style laws (CLAUDE.md)
No `eprintln!`; `tracing` only. Comments state only constraints the code cannot show; no change-log narrative. No em dashes. Banned words in prose and identifiers: provenance, substrate, load-bearing, regime, ground truth (say oracle), refusal, support (say refCount). `emit_ts.pl` output for unchanged programs stays byte-identical (tsv2 is paused).

Done: `boop beep hail sprefa-coordinator --from <your lane> --body "PR #<n>: <numbers>"`; if refused, message the session named sprefa-* over the cross-session socket.
Blocked or brief wrong: one line, stop.
