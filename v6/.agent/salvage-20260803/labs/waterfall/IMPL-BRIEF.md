# brief: implement the session waterfall from PLAN.md

Implementation lane. FIRST ACTION: `git merge --ff-only 97afe29` (this
worktree, branch lab/waterfall-plan); failure = STOP. Then follow
PLAN.md at this root as the spec, step ladder in order, each step
committed with green gates before the next. If PLAN.md contradicts the
actual codebase anywhere, STOP and write the contradiction into
REPORT.md; do not improvise around it.

Bounds: this worktree only. New code lives in
src/plugins/harnessTrace/ following the 0_/pure + component split;
interfaces I-prefixed in 0_types.ts. The "Show active" checkbox
DEFAULT CHECKED = today's exact strip; unchecking = waterfall + top
range brush + range-constrained table (user's words in PLAN.md).
Lazy law: message events load per visible range, never all history.

Gates per step: npx vitest run src/plugins/harnessTrace (all pass,
new pure fns tested), npx tsc --noEmit (no NEW errors;
plugin.test.ts CtxItem error is pre-existing), and for the final step
a playwright spec on the stripsub pattern (seeded rows, screenshot of
the waterfall). Paste all outputs into REPORT.md with commit shas.
Style: banned words provenance/substrate/load-bearing/regime; comment
budget constraints-only. No pushes, no subagents.
