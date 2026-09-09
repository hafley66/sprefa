# Lane `fix-extract-scip-speed-3` (glm53f): the scip-informed leg under 10 s

Read `plans/extract-bench-2026-08-29/SCIP-SPEED.BRIEF.md` (Tasks A, B, C are
the job) and the dead lane's draft
`/Users/chrishafley/projects/sprefa-worktrees/briefs/plans/extract-bench-2026-08-29/scip-speed-2.dead-lane.REPORT.draft.md`. Two earlier lanes were killed
before posting: 4 commits on `origin/fix/extract-scip-speed` and one
uncommitted patch. The machine died once under parallel gates; every extract
run here is `nice -n 15`, one at a time, `timeout 60`.

## First action
```
git merge --ff-only 292888ba57340de0cc72219274e3a0091b55bbb3
git merge --no-edit origin/fix/extract-scip-speed
git apply --3way /Users/chrishafley/projects/sprefa-worktrees/briefs/plans/extract-bench-2026-08-29/scip-speed-2.dead-lane.scip.rs.patch
cd v6/sprefa-extract && nice -n 15 cargo build --release --features cli 2>&1 | tail -1
git log --oneline origin/main..HEAD
```
If the apply conflicts, keep the patch's intent (per-document sorted
occurrence vectors, symbol->def map, interned symbol ids) and paste the
hunks in the PR body. Commit the applied patch as
"scip: index the index (recovered from the killed lane)". Then write
`SCIP-SPEED.REPORT.md` section 1: which of A/B/C each inherited commit and
the patch already cover.

## Then, in order, one commit per receipt
Task A wall receipt: 3 runs per corpus (ts, rust, go) with the index,
ts and rust under 10 s, go reported; `sort a.jsonl | cmp - <(sort b.jsonl)`
identical before/after on every corpus (build the before jsonl from
origin/main's binary in a separate `target` dir, or from the committed
`out/` tsvs if present). Task B: fix the go rows in `out/scip_runs.sh` and
rerun the go table. Task C: informed-by-default when
`scip_freshness` says fresh, one `tracing::info` line naming the choice,
fail-first test; then `RATCHET_BUMP=1 just extract-ratchet` for ts5 and
rust call rows only.

## Ownership
`src/scip*.rs`, `src/bin/extract.rs` (flag plumbing only),
`tests/scip_freshness.rs`, `tests/8_scip_families_cli.rs`,
`tests/74_scip_relationship_family.rs`, `SCIP.REPORT.md`, `SCIP-SPEED.REPORT.md`,
`out/scip_runs.sh`, RATCHET.tsv ts5/rust call rows with BUMP only. NOT
`src/lang/*`, `src/project.rs`, `src/types.rs`, `tests/bench/mod.rs`. No
`cargo fmt` on files you do not own. Gate (`nice -n 15 cargo test --release
--features cli`) in background with a log, never while a bench run is
measuring; wall-ratio flakes rerun 3x isolated. No file over 1 MB. Budget 90
min; past it, post the PR with Task A.

Push `fix/extract-scip-speed-3`, `gh pr create --base main`, hail
`boop beep --no-wait --as fix-extract-scip-speed-3 sprefa-coordinator "scip speed 3: PR #N, informed wall ts a s rust b s go c s, tasks A/B/C done, gate x/y"`.
Laws: no em dashes anywhere, no eprintln (tracing only), descriptive names,
comments only for what code cannot show, no words
provenance/substrate/load-bearing/regime/refusal, never "ground truth".
