# tick-transaction

Issue: `issuectl show tick-transaction`. Base: `git merge --ff-only <sha the coordinator states>` first; fail = stop and hail. Branch `fix/tick-transaction`. PR to main.

## The defect
A tick is many autocommit statements. A process killed mid-tick leaves base tables half-promoted; since #423 a level whose inputs did not move again is not recomputed, so the half state can persist past the first tick after restart only because `ordered.rs::first_fold` rebuilds everything once. The proper rail is one SQLite transaction per tick.

## Deliverable
1. `src/sql.rs`: `SqliteSeam::begin_tick()` / `commit_tick()` / `rollback_tick()` (`BEGIN IMMEDIATE`, `COMMIT`, `ROLLBACK`), counted in `SEAM_TALLY` like any statement. Nested begin is a named error, never silent.
2. Call sites: `src/driver.rs` `run_schedule`/`run_schedule_live` and `src/run.rs` resident loop (`LiveLoop::fold`) wrap each tick; an `Err` from the tick rolls back and propagates. The one-tick-path lane owns `program.rs`, `ordered.rs`, `incremental.rs`: do not touch them; the wrap goes around `drive_tick`/`program.tick` from the driver side only.
3. Test, fail-pre-fix: `tests/tick_transaction.rs` folds a small program against a FILE db (`DL_DB_URL` door, see `sql.rs:46`), injects a failure mid-tick through a host executor that returns `Err` on its second call (see `executors/` for the `HostRunner` trait; a test-only executor is fine), reopens the file, asserts every rel equals the previous tick's state byte for byte. At base the assertion fails (half state visible). Also assert `SEAM_TALLY.statements` per tick rises by exactly 2 (BEGIN, COMMIT) on the ghcache schedule.
4. `docs/failure-modes.md` entry (next free number): incident = #423's "Left on the table" SIGKILL note, RCA, fail-pre-fix, rail, entry.
5. ghcache gate and goldens byte-identical (a transaction changes no row).

## You own
`v6/sprefa-engine-rs/src/{sql.rs,driver.rs,run.rs}`, `v6/sprefa-engine-rs/tests/tick_transaction.rs`, `docs/failure-modes.md`.
Forbidden: `program.rs`, `ordered.rs`, `incremental.rs`, `emit_rust.pl`, everything under `v6/prolog`, `v6/dl`. In `sql.rs` do not move `explain_once` or the tally; add beside them.

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

## Continue, do not restart
The branch `origin/fix/tick-transaction` exists with the work done through item 4 (head da9d4b44b, PR #428 open). Check it out, `git merge origin/main` (main moved three times: #427 deleted ordered.rs, #429, #430), resolve, re-run the seven gates, push, update the PR body with the numbers and the RSS gate window old/new with the measured RSS series, hail.
