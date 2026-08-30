# DL7 expression-lowering blast radius

Work only inside the lane worktree created from base `606379b98`. Read
`plans/2026-08-30-dl7-relational-expression-flow.md` first.

Produce `v7/tasks/results/16_EXPRESSION_BLAST_RADIUS.md`. Do not edit production
code or tests.

Report exact predicate call paths, current data shapes, source locations, test
assertions, and expected changed files for milestones 1 through 8. Include:

- `lower_bind`, `lower_target`, `lower_call_mode`, and `lower_argument` callers;
- reservation and name-resolution dependencies;
- origin tracking and diagnostics;
- return-edge discovery from canonical `:/4` rows;
- interaction with prelude loading, compiler refreeze, and generated rules;
- every occurrence of `partial_request`;
- the smallest focused test command and current count.

Commit the report once with subject `Map DL7 expression lowering blast radius`.
Run `git diff --check`; no production suite run is required.
