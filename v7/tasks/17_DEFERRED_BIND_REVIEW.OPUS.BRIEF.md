# Review deferred expression binds

## Base

- Worktree: `/Users/chrishafley/projects/sprefa-worktrees/dl7-count-aggregate`
- Branch base: current `feature/dl7-count-aggregate`
- Plan: `plans/2026-08-30-dl7-relational-expression-flow.md`
- Prior reviews:
  - `v7/tasks/results/15_EXPRESSION_FLOW_REVIEW.md`
  - `v7/tasks/results/16_EXPRESSION_BLAST_RADIUS.md`

## Fixed language decisions

- Full relation calls retain every tuple position and reverse-query behavior.
- An expression call omits one declared `return` position.
- Expression lowering must erase to ordinary first-order Datalog goals.
- A userland relation may compute a result that cannot be predicted structurally.
- Same-unit references to a derived bind must work after compiler refreeze.
- Do not introduce a `request` relation or a dynamic runtime apply operator.

## Proposed representation

1. Declaration lowering recognizes `(: Name (Call ...))` and records a
   `derived_reservation(Owner, Name, BindNodeId, Index)` plus a declaration
   marker that participates in duplicate-name and dense-index validation.
2. It emits no static `pending_edge/4` for that bind.
3. Executable lowering turns the bind into an authored rule:

   ```text
   :(Owner, Name, Result, Index) <- Call(..., Result).
   ```

4. A bare atom referring to a derived reservation lowers to a fresh value
   variable plus a positive lookup goal:

   ```text
   :(Owner, Name, Value, Index)
   ```

5. Static atom references keep the existing declaration-time resolution.
6. Derived names cannot occupy the relation-operator position until the
   compile-known partial-application milestone supplies an erasing rewrite.
7. Generated lookup goals are ordinary checked kernel `:/4` calls. Refreeze
   makes a newly derived edge visible on the following compiler round.

## Review questions

Read the relevant lowerer, checker, evaluator, compiler-round, prelude, and
entrypoint tests. Return a concise review with exact file and predicate
references:

1. Does the representation remain ground in the compiler IR?
2. Can duplicate labels and dense indices be checked without inventing a
   second semantic edge relation?
3. Does rule ordering permit chained binds such as
   `(: B (Option A))` after `(: A (Partial User))`?
4. Does `:/4` refreeze create any functional-key collision or unstable round?
5. Which exact predicates need edits for milestones 1 through 5?
6. What is the smallest alternative if this representation conflicts with an
   existing invariant?

## Deliverable

Write only `v7/tasks/results/17_DEFERRED_BIND_REVIEW.md`. Do not edit
production code or tests. Commit with subject:

`Review deferred DL7 expression binds`

