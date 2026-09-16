# Lane A: compute and emit ruleObservers (prolog side)

FIRST ACTION: `cd /Users/chrishafley/projects/sprefa-lanes/skipA && git merge --ff-only 4c1791a8`. If it fails, STOP and report. If reality deviates from this brief at any point (a cited line is wrong, a command is missing), STOP and report; do not improvise.

## Task
Implement EXACTLY lane A of `plans/2026-08-06-unread-rel-skip-contract.md` section 8 (the file is in your tree; read sections 4a, 8, 9 before editing anything):

1. `v6/prolog/analyze.pl`: predicate `rel_rule_observers(+Rules, +Ref, -HeadRefs)`, the five reader clauses of contract section 4a, built on `0_body_walk.pl`'s registry walk, result sorted. Export it.
2. `v6/prolog/emit_ts.pl`: render `ruleObservers: ["h/2", ...]` on each `IIncrementalRelationPlan` entry line (the entry template near emit_ts.pl:906-910), following the optional-field precedent of `departureFrontierTableName` at :896-905. The TypeScript type for the field already exists (`v6/tsv2/runtime/types.ts:119-121`, landed at your base sha); emit the field on EVERY relation entry, empty array when no rule observes.
3. plunit tests in the prolog test suite (`v6/prolog/compile/test/plunit_tests.pl`): the predicate agrees with a hand-written expectation on one fixture per reader family (level body ref, edge trigger, finalize, aggregate delta ref, ordered carry).

## Ownership
You own ONLY: `v6/prolog/analyze.pl`, `v6/prolog/emit_ts.pl`, `v6/prolog/compile/test/plunit_tests.pl`, plus regenerated outputs under `v6/prolog/compile/out/` and `v6/tsv2/gen_emitted/` produced by the sweep. You do NOT open `v6/prolog/lower.pl`, any file under `v6/tsv2/runtime/`, or any SQL text. Touching SQL means the tick logs change and your gate fails.

## Gates, run all, paste real numbers in your report
- plunit: from `v6/prolog/compile`: `swipl -g run_tests -t halt test/plunit_tests.pl` (346 at base; yours add to that).
- Sweep: `cd v6/tsv2 && bash scripts/sweep.sh` — 420 fixtures, wrong=0. Your field is additive JSON on the entry line; tick logs must stay byte-identical.
- Typecheck: from `v6/tsv2`: `pnpm exec tsgo --noEmit` (0 errors at base).
- This repo is pnpm. Never run npm install; never touch any lockfile.

## Style laws
- Comments state only constraints the code cannot show; max 2 consecutive comment lines; no change-log narrative, no dates.
- Banned words in prose and identifiers: provenance, substrate, load-bearing, regime, support (use refCount vocabulary).
- Follow each file's existing style exactly.

## Commit
Commit on branch `lane/skip-observers` with a clear message. Never push, never spawn agents, never touch other worktrees. Write REPORT.md at the worktree root: gate table with numbers, the predicate's clauses as landed, deviations if any.
