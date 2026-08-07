# Lane C: the skip contract's rails

FIRST ACTION: `cd /Users/chrishafley/projects/sprefa-lanes/laneC && git merge --ff-only e1a9696f`. If it fails, STOP and report. If reality deviates from this brief at any point (a cited file or section is missing, a gate command does not exist), STOP and report; do not improvise. This is pass 1 of 2; a separate review pass audits your work, so favor plain obvious code over cleverness.

## Task
Implement EXACTLY lane C of `plans/2026-08-06-unread-rel-skip-contract.md` section 8 (in your tree; read the whole contract first, especially the rails section and section 4). Lanes A and B are already merged at your base: `ruleObservers` is emitted on every relation entry (analyze.pl/emit_ts.pl) and the runtime skip + carry signal exist in `v6/tsv2/runtime/1_incremental.ts` (`isUnobserved`, `observedRels`).

1. RAIL A, the text audit, in `v6/tsv2/scripts/` (name it per the contract, or `unobserved-rel-audit.mjs` if the contract does not name it): for every rel in every compiled module under `v6/tsv2/gen_emitted/` whose `ruleObservers` is empty, scan the module TEXT for every occurrence of its `__delta_<rel>` and `__frontier_<rel>`/`__next_frontier_<rel>` table names and require each occurrence to be one of: DDL, a clear (`DELETE FROM` whole-table), a known writer (`INSERT INTO`), or the boundary SELECT. Any other read of a skipped rel's staging = exit 2 with the module, rel, and offending statement printed. Exit 0 otherwise, printing a one-line count summary.
2. `v6/tsv2/tests/nolistenCounts.test.ts` (node:test runner, follow the style of `tests/recursiveClosureCounts.test.ts` exactly): a statement-COUNT test proving the skip fires. Build a two-rel program seam in-memory the way the closure counts test does, set `seam.unobservedRels` for a rel with empty `ruleObservers`, and pin: (a) statements per tick with the skip active < statements with it inactive by exactly the number of copy statements the contract enumerates, and (b) final head state identical in both runs. Use a wrapped runner to count statements, the `recordingRunner` pattern from `v6/labs/exec_shootout/dl6/bench.ts:82-110`.
3. Fail-first receipt: before wiring the audit into its final form, temporarily plant a fake read of a skipped rel's delta table in a scratch copy of one module under your scratch dir (NOT in gen_emitted), run RAIL A against it, and paste the failing output into the test/script header comment as the sabotage receipt. gen_emitted stays untouched by hand.

## Ownership
You own ONLY: the new script in `v6/tsv2/scripts/` and `v6/tsv2/tests/nolistenCounts.test.ts`. You do not edit runtime files, .pl files, gen_emitted, or existing tests.

## Gates, run all, paste numbers
- `node <your rail script>` from v6/tsv2: exit 0 over all compiled modules, summary line printed.
- From v6/tsv2: `pnpm test` (node --test): all pass, your new test included, no skips added.
- `pnpm exec tsgo --noEmit`: 0 errors.
- Repo is pnpm; never npm, never touch lockfiles.

## Style laws
- Comments: max 2 consecutive lines, only constraints code cannot show; sabotage receipts in TEST headers are required and exempt.
- Banned words in prose and identifiers: provenance, substrate, load-bearing, regime, support.
- Interfaces in types files carry the I prefix; follow existing file style.

## Commit and report
Commit on `lane/skip-rails`. If the pre-commit comment rail demands a missing cargo binary, build it exactly as the hook's message instructs (`cargo build --release --features cli --bin extract` in v6/sprefa-extract) and note that in the report. Never push, never spawn agents. REPORT.md at worktree root: gate table with numbers, the rail's writer/DDL/clear allowlist as landed, the sabotage receipt, deviations.
