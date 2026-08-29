# GLM53F brief: lower nested binds and root forms

Read `v7/design/2_BASEMENT_TO_DATALOG.PLAN.md` first and implement milestone
2 only. Milestone 1 will already be present in the base commit.

## Scope

- Work only in the assigned worktree.
- Add `v7/1_DATALOG/0_lower.pl` and update
  `v7/tasks/00_PROGRESS.md` only.
- Export exactly `lower_datalog/4`.
- Implement the three-pass owner, reserve, and lower algorithm and exact
  `basement_program/2` terms from the plan.
- Make every nested `:` a pending edge. Make every nested `*` and `+` an owner
  with one parent-scope row.
- Require explicit binds. Add no declaration inference.
- Reify variables as ground `var(Identity)` terms.
- Keep atoms as pending names, literals as constants, and constructor owners
  as targets. Perform no reference resolution, type evaluation, application
  lowering, interning, or fixpoint work.
- Add no test file.

## Gate

Run one direct SWI command that loads the existing fixture, lowers it, prints
the canonical program, and exits nonzero on diagnostics. Record that exact
command and result in the commit body. Run `git diff --check`. Run no suite.

## Commit

Create at least one commit with exact subject:

```text
v7: lower nested root forms
```

Add trailer `Refs-Issue: @dl7-datalog-lower`. Do not push. Stop on any source
form outside the plan instead of widening the grammar.
