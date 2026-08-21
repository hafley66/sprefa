# Bind runtime reconciliation

Read-only audit first. Work from current main.

Claims to verify:

1. `bind` allegedly "dies Phase 4".
2. Anonymous sum values allegedly remain only in live worktrees.
3. `option(enum)` allegedly remains spelling-only.

For `bind`, trace one real declaration from parser through expansion, plan,
emitter, TypeScript runtime, and Rust runtime. Run the smallest existing CI
that proves or disproves actual delivery of an interval or watch bind row.
Distinguish `:=` expression binding from `bind Name(...)` world-source
declarations. Report exact files, predicates/functions, and test names.

For anonymous sums and option(enum), compare current main commits/tests with
the two `/private/tmp/sprefa-anonymous-sum-*` worktrees. Do not merge or delete
anything. Report whether either worktree contains commits absent from main.

Do not implement speculative features. If `bind` has a real defect, provide a
minimal fail-first reproduction and identify the narrow repair site. Commit a
test/fix only when the defect is reproduced. Run project CI relevant to new
work. Send completion using `boop tell-parent --kind completion`.
