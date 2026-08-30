# DL7 relational expression-flow review

Work only inside the lane worktree created from base `606379b98`. Read
`plans/2026-08-30-dl7-relational-expression-flow.md`, favorites summarized in
`v7/design/0_KERNEL_RECONCILIATION.md`, and the current V7 lowerer, checker,
compiler, prelude, fixture, and consolidated tests.

Produce `v7/tasks/results/15_EXPRESSION_FLOW_REVIEW.md`. Do not edit production
code or tests.

The report must:

1. Trace the exact current call and bind lowering path by predicate and line.
2. Test the plan against full relational reverse queries, body ordering,
   functional keys, generated compiler rounds, and SQL-lowerable first-order
   Datalog.
3. Identify any milestone whose proposed representation cannot fit the current
   checked IR.
4. Give exact signatures and pseudocode bodies for the smallest milestones 1
   through 4 implementation.
5. Identify collisions with `count` nested head syntax and generated-program
   assembly.
6. State every open choice. Do not select syntax that the plan leaves open.

Commit the report once with subject `Review DL7 relational expression flow`.
Run `git diff --check`; no suite run is required.
