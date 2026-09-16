# Brief (v3): on feature/arrivals-and-ticks, a program routed to a continuing executor stays resident

FIRST ACTIONS: `git fetch origin feature/arrivals-and-ticks && git reset --hard
origin/feature/arrivals-and-ticks` (ce1130f53, the collapse, 26 commits, gated by the
coordinator); `bash v6/tools/doctor-deps.sh` (DEPS OK). Never spawn subagents. Commit every
green step. Push `--force-with-lease` to the same branch; PR #408 is the PR.

## Coordinator's gate on ce1130f53
conformance 440, plunit 1042/0, grade 440/335, ghcacher 6, oracle-rustc OK, oracle-knip OK.
ENGINE: 2 failures in `tests/dl6_run.rs`:
- `:366 one_touched_file_produces_exactly_one_extra_tick`: a touched file produced no tick.
- `:434 a_resident_run_measures_itself_and_a_storeless_program_stays_flat`: 0 cost rows in 8s.
Cause, read it: `src/run.rs:702 stays_resident(binds)` asks for a `bind watch`/`bind interval`
decl (`WATCH_EXECUTOR`, `INTERVAL_EXECUTOR`, `bind_rel`), and the collapse deleted `bind`.
The re-spelled programs route `/soopy/watch` and `/clock/tick` rels to executors whose
`cadence()` is `ExecutorCadence::Continuing` (`hosts.rs`, grep `Continuing`).

## What the previous lane measured before its turn ended (its finding stands)
`hosts.rs` on the branch has NO `ExecutorCadence` and NO executor behind `/clock/tick` or
`/soopy/watch`: the collapse renamed the surface but the continuing executors were never
built on this branch (they existed only in a salvage that was lost; `run.rs` from #407
still drives `live_watch`/`live_interval` through `bind` plans). So the brief's "grep
Continuing" line was wrong. Build them:
- `src/executors/clock.rs` `ClockExecutor`: input `every: int`, output `bucket: int` =
  `floor(epoch_secs / every)`; re-answers when the bucket turns over.
- `src/executors/watch.rs` `SoopyWatchExecutor`: input `glob`, output `(path, digest)` via
  `soopy::SourceTree::open(repo).watch(SourceQuery{Worktree, patterns})`, one
  `soopy::SourceWatcher` per (root, glob) kept for the runtime's life; the watcher wakes,
  `soopy::enumerate` answers.
- `IHostExecutor` gains `fn cadence(&self) -> ExecutorCadence` with a default of `Once`;
  the two above return `Continuing`. Register `/clock/tick` and `/soopy/watch` in
  `LINKED_EXECUTORS`, `executor_for`, and `registry.pl arrival_executor/2`.

## Fix
`stays_resident` and every `bind_rel(...)` reader in `run.rs` decide from the loaded
program's routed executors: resident iff any routed rel's executor reports
`ExecutorCadence::Continuing`. The watch loop's `Moved` arm and the clock loop take the
glob / period from that rel's demand rows, not from a bind decl. Delete `BindPlanData`
readers that have no caller left. The fixtures those two tests load
(`v6/dl/fixtures/served-watch-rail.dl6`, the self-measurement program) must already be in
the rel form on the branch; if not, re-spell them.

## Also on this branch
- `grep -rlE '^(sh|bind) ' v6/dl v6/prolog/conformance/fixtures` still lists
  `v6/dl/fixtures/pr-size.dl6`, `v6/dl/fixtures/sg-rail.dl6`,
  `v6/prolog/conformance/fixtures/one_rel_with_arrivals_probe.dl6`. Re-spell the first two;
  the probe fixture is the PLAN's own before/after probe, keep it only if its manifest row
  expects `removed_word`, else re-spell it too.
- `v6/dl/prwatch/prwatch.dl6` in the rel form with the four `repo(...)` seeds from main.

## Gate, three runs for the first two, paste in a PR #408 comment
cd v6/sprefa-engine-rs && cargo build --release --bin dl6 && timeout 900 cargo test -q   # must be N/0
cd v6/prolog/conformance && timeout 600 swipl -g go -t halt go.pl | grep -c '^PASS'      # 440
cd v6 && just plunit && bash ../v6/sprefa-engine-rs/grade.sh                             # 1042/0, 440/335
cd v6 && just oracle-rustc && just oracle-knip && just ghcacher-rust && just feature-reach && just crosswalk-gate && just v5-rails && just selfdoc-check
Then `gh pr view 408 --json mergeable` = MERGEABLE.

## Ownership
Yours: `src/run.rs`, `src/bin/dl6.rs`, `tests/dl6_run.rs`, the three fixture files above,
`v6/dl/prwatch/prwatch.dl6`, `hosts.rs` cadence plumbing only. FORBIDDEN: `v6/prolog/3_clock_check.pl`,
`v6/dl/ghcache/**` (another lane), `src/executors/**` except reading `cadence()`, `v6/tsv2/**`.

## Style laws
No em dashes. Banned: provenance, substrate, load-bearing, regime, refusal, "ground truth".
tracing only. Comment budget: constraints only. Failure ledger entry: "a keyword deleted
from the surface while the runtime still keyed on it".

## Reaching the coordinator
`boop beep hail sprefa-coordinator --from <your-lane-name> --body "<one line>"` lands in the
coordinator hook inbox at its next turn. Use it when blocked, when done (PR number + gate
numbers), when this brief is wrong, or when you find a defect outside your ownership.

## Turn law (boop treats the end of your turn as your death)
Never end your turn before the PR is posted and its gate numbers are in a `boop beep hail
sprefa-coordinator` body. Waiting on a background job: poll it with an `until` loop inside
one Bash call, never by ending the turn. A finding that changes scope: hail it in one line
AND keep working on the parts it does not change.
