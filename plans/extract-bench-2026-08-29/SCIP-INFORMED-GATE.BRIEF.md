# Lane `fix-extract-scip-informed-gate` (glm53f): land PR #585 green

PR #585 (`origin/bench/extract-scip-informed`, 3 commits) failed 2 gate targets at
`~/projects/sprefa-worktrees/gate-585.log`. Your job: rebase it on origin/main,
fix both, run the full extract gate, push, hail. No new features.

## First action
```
git merge --ff-only aa95c0ef361e305f362005b09d0fbabaa75afca7
git merge --no-edit origin/bench/extract-scip-informed
cd v6/sprefa-extract && cargo build --release --features cli 2>&1 | tail -1
```
If the merge conflicts, resolve keeping BOTH sides' intent, and paste the
conflicting hunks in the PR body.

## Failure 1: `tests/6_kind_vocab.rs:243` wire byte count drifted
`wire_output_is_byte_identical_to_the_946460d75_golden`: left 2197731, right
2194631 (golden). #585 added relationship rows to the wire on purpose (commit
4084f4abb "the family stream emits the raw relationship table"). Regenerate
`tests/fixtures/kind_vocab/wire_golden.jsonl` the way the test header says
(read lines 1-30 and 205-245 of the test file for the regen command and the
corpus pin `corpus.txt`). Then `diff <(sort old) <(sort new)` and paste the
COUNT of added lines and 3 sample added lines in the PR body. If ANY line was
removed or changed rather than added, STOP and hail; that is a regression.

## Failure 2: `tests/45_emit_throughput.rs:89` piped emission 6.19 s for 350005 lines
The budget constant `WALL_BUDGET_SECS` is near line 20-40. Run the test
alone, 3 times, machine idle:
```
for i in 1 2 3; do timeout 60 cargo test --release --features cli --test 45_emit_throughput -- --nocapture 2>&1 | grep -E 'took|test result'; done
```
If it passes 3/3 isolated, it was lane load; record the three walls in the PR
body and do not touch the budget. If it fails isolated, compare
`git diff origin/main -- src/` for anything on the emit path (grep
`BufWriter`, `writeln`, `serde_json::to_writer` in the diff) and fix the
#585 change that slowed it. Never raise the budget.

## Gate
```
cd v6/sprefa-extract && (cargo test --release --features cli 2>&1 | tee /tmp/gate-585b.log | tail -3) &
```
Poll the log every 60 s; never foreground-wait. Report `error: N targets failed`
or the summary. Then `cd v6 && just extract-ratchet` in background with a log;
ratchet rows must not drop (RATCHET.tsv is the floor file).

## Ownership
`tests/6_kind_vocab.rs`, `tests/fixtures/kind_vocab/wire_golden.jsonl`,
`tests/45_emit_throughput.rs`, `src/scip*.rs`, `src/bin/extract.rs`,
`plans/extract-bench-2026-08-29/SCIP.REPORT.md`. NOT `src/lang/*` (a ts lane
is live), NOT `RATCHET.tsv`. No `cargo fmt` on files you do not own. No file
over 1 MB. Budget 40 min.

## Deliver
`git push origin HEAD:bench/extract-scip-informed` (updates PR #585), then
`gh pr edit 585 --body-file <file>` appending a "Gate fix" section: golden
added-line count, throughput walls x3, gate summary, ratchet summary. Hail:
`boop beep --no-wait --as fix-extract-scip-informed-gate sprefa-coordinator "585 gate: golden +N lines, throughput a/b/c s, gate <summary>, ratchet <summary>"`
Laws: no em dashes anywhere, no eprintln (tracing only), descriptive names,
comments only for what code cannot show, no words
provenance/substrate/load-bearing/regime/refusal, never "ground truth".
