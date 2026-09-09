# Lane `fix-extract-scip-speed-2` (glm53f): resume the scip-index speed work

Read `plans/extract-bench-2026-08-29/SCIP-SPEED.BRIEF.md` first: Tasks A (index
the index), B (go scip normal form), C (informed-by-default when fresh) are the
whole job. A prior lane pushed 4 commits on `origin/fix/extract-scip-speed`
and was stopped. #585 has since merged (relationship rows on the wire); base
is origin/main at 619724c7f13d7de295f433209dea0cc07ea6ca87.

## First action
```
git merge --ff-only 619724c7f13d7de295f433209dea0cc07ea6ca87
git merge --no-edit origin/fix/extract-scip-speed
cd v6/sprefa-extract && cargo build --release --features cli 2>&1 | tail -1
git log --oneline origin/main..HEAD
```
If the merge conflicts, resolve keeping both sides' intent and paste the
hunks in the PR body. Then read the 4 inherited commits' diffs and state in
`plans/extract-bench-2026-08-29/SCIP-SPEED.REPORT.md` section 1 which of
Tasks A/B/C each commit already covers, with the measured wall if any.

## Then
Finish Task A first (the wall receipt: 3 runs per corpus, ts and rust under
10 s, go reported, and `sort a.jsonl | cmp - <(sort b.jsonl)` identical before
and after on every corpus). Commit. Then B, then C, each with its receipt.
Every extract call under `timeout 60`; a run over 60 s is killed and its
partial number reported.

## Ownership
Same as SCIP-SPEED.BRIEF.md: `src/scip*.rs`, `src/bin/extract.rs` (flag
plumbing only), `tests/scip_freshness.rs`, `tests/8_scip_families_cli.rs`,
`tests/74_scip_relationship_family.rs`, `SCIP.REPORT.md`, `SCIP-SPEED.REPORT.md`,
`out/scip_runs.sh`. RATCHET.tsv: ts5 and rust call rows only in Task C, and
ONLY with RATCHET_BUMP=1 (never FORCE). NOT `src/lang/*` (a ts lane is
live), NOT `src/project.rs`, `src/types.rs`, `tests/bench/mod.rs`. No
`cargo fmt` on files you do not own. Gate in background with a log;
wall-ratio flakes rerun 3x isolated. No file over 1 MB. Budget 90 min; past
it, post the PR with what is green.

Push `fix/extract-scip-speed-2`, `gh pr create --base main`, hail
`boop beep --no-wait --as fix-extract-scip-speed-2 sprefa-coordinator "scip speed 2: PR #N, informed wall ts a s rust b s go c s, tasks done A/B/C, gate x/y"`.
Laws: no em dashes anywhere, no eprintln (tracing only), descriptive names,
comments only for what code cannot show, no words
provenance/substrate/load-bearing/regime/refusal, never "ground truth".
