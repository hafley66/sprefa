# Lane B: the runtime skip and the carry signal (typescript side)

FIRST ACTION: `cd /Users/chrishafley/projects/sprefa-lanes/skipB && git merge --ff-only 4c1791a8`. If it fails, STOP and report. If reality deviates from this brief at any point (a cited line is wrong, a command is missing), STOP and report; do not improvise.

## Task
Implement EXACTLY lane B of `plans/2026-08-06-unread-rel-skip-contract.md` section 8 (the file is in your tree; read sections 4b, 5, 8, 9 before editing anything):

1. `v6/tsv2/runtime/types.ts`: rename `unreadRels` to `unobservedRels` on `ISqlSeam` (types.ts:66). Call sites to update: `v6/tsv2/runtime/1_incremental.ts:1090`, `v6/labs/exec_shootout/dl6/bench.ts:220`, `v6/labs/exec_shootout/dl6/run.ts:159`, and `v6/labs/exec_shootout/dl6/incbench.ts:19`. The `ruleObservers?: readonly string[]` field on `IIncrementalRelationPlan` already exists at your base sha (types.ts:119-121); use it, do not re-add it.
2. `v6/tsv2/runtime/1_incremental.ts`: one predicate `isUnobserved(relation, seam)` = `(relation.ruleObservers ?? ["*"]).length === 0 && seam.unobservedRels?.has(relation.rel) === true`. The `?? ["*"]` fail-safe stays exactly as written: a module without the field is never skipped.
3. Apply it at the ten writer sites of contract section 4b, plus exclude skipped rels from the `retractionGuardSql` term list.
4. The carry signal of contract section 5: `carryPending` stops reading EXISTS on a skipped rel's `__next_frontier_` and uses the fill statement's `rowsAffected` instead, per the precedent at 1_incremental.ts:589 (`results[1]!.rowsAffected`).
5. With `seam.unobservedRels` absent or empty, every code path returns the same arrays by reference as today (the `3_subscribe.ts:52-56` move the contract cites).

NOTE: `1_incremental.ts` gained a `maintainHeadInPlace` function and an `IDredPlan` driver at your base sha. The contract was written one commit before that landed; where the contract's cited line numbers are off by the new code, re-locate by symbol name, and the skip must ALSO cover the arrival staging the dred driver performs (`arrivalASql`/`arrivalBSql` stage into `__new_`, then `arrivalTail` copies to delta/frontier: the copies are the skippable half, the `__new_` fill is the carry signal source). If that interaction is unclear after reading both, STOP and report rather than guessing.

## Ownership
You own ONLY: `v6/tsv2/runtime/types.ts`, `v6/tsv2/runtime/1_incremental.ts`, and the three lab files named in item 1 for the rename alone. No `.pl` files, no gen_emitted, no tests directory (lane C owns the rails).

## Gates, run all, paste real numbers in your report
- Typecheck: from `v6/tsv2`: `pnpm exec tsgo --noEmit` (0 errors).
- Test battery: from `v6/tsv2`: `pnpm test` (node --test; 150 files / 149 pass / 1 skip at base).
- Sweep: `cd v6/tsv2 && bash scripts/sweep.sh` — 420 fixtures, wrong=0, byte-identical: no module carries a nonempty boot skip set today, so behavior must not change anywhere.
- Bench sanity: `cd v6/labs/exec_shootout/dl6 && bash bench.sh` completes with the same checksums it prints at base (grid case is enough; the full run is optional).
- This repo is pnpm. Never run npm install; never touch any lockfile.

## Style laws
- Comments state only constraints the code cannot show; max 2 consecutive comment lines; no change-log narrative, no dates.
- Banned words in prose and identifiers: provenance, substrate, load-bearing, regime, support (use refCount vocabulary).
- Async stays rxjs Observable above the SqlRunner seam; never `await` an Observable.
- Follow each file's existing style exactly.

## Commit
Commit on branch `lane/skip-runtime` with a clear message. Never push, never spawn agents, never touch other worktrees. Write REPORT.md at the worktree root: gate table with numbers, the writer-site list as landed, deviations if any.
