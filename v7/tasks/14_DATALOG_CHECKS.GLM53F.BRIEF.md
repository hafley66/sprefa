# GLM53F brief: resolve, check, and graph lowered Datalog

Read `v7/design/2_BASEMENT_TO_DATALOG.PLAN.md` first and implement milestone
3 only. Milestones 1 and 2 will already be present in the base commit.

## Scope

- Work only in the assigned worktree.
- Extend `v7/src/2_comptime/0_compiler.pl` and update
  `v7/tasks/00_PROGRESS.md` only.
- Add `check_datalog/4` to the existing module exports.
- Resolve pending names through owner edges and reverse binding edges, with the four
  pinned primitive references from the plan.
- Emit canonical `':'/4` edges and replace call names with `ref(Target)`.
- Validate binds, dense indices, explicit relation use, arities, ground seeds,
  and positive-rule safety.
- Emit distinct positive dependency rows and one SCC stratum row per declared
  relation.
- Return deterministic origin-ordered diagnostics and no checked value on
  failure.
- Add no evaluator, dynamic clauses, tabling, negation, aggregate, type rule,
  interning behavior, or test file.

## Gate

Run one direct SWI command covering nested product and sum edges, parent-edge
resolution, a recursive graph, undeclared use, arity mismatch, and unsafe head
variable. Record exact observed terms in the commit body. Run
`git diff --check`. Run no suite.

## Commit

Create at least one commit with exact subject:

```text
v7: check lowered Datalog basement
```

Add trailer `Refs-Issue: @dl7-datalog-checks`. Do not push. Stop if the graph
requires a semantic rule beyond positive Datalog.
