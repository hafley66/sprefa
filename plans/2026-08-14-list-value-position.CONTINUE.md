# Continuation brief: list(T) value position, slices 2-4

The full contract is
`/Users/chrishafley/projects/sprefa/plans/2026-08-14-list-value-position.PLAN.md`
(main tree, absolute path — it is not in your worktree). Read it first; it
governs. This file only states where the previous lane stopped.

## First action
```
git merge --ff-only 74d05b484bb2b63ff8f43e3d91c6e24618e653a7
```
Failure = STOP AND REPORT. That sha already contains slice 1.

## State you inherit

Slice 1 is LANDED at `74d05b48` and validated (conformance 421/0, plunit 5
known-red, sweep 317/314/0, RUST-GRADE 421/313): the `list(T)` entity mint now
carries `content` (text, UNIQUE) instead of `id`; 9 fixtures re-sent canonical
json text; plunit pin updated.

Slice 2 exists as an UNVALIDATED partial patch at
`/Users/chrishafley/projects/sprefa/plans/2026-08-14-list-value-position.slice2-wip.patch`
(274 lines, main tree, absolute path). It was rejected by the pre-commit rail because the
half-edited `lower.pl` calls `maplist/6`, which `lower.pl`'s import list does
not cover. Treat the patch as REFERENCE: apply it, fix it, or redo it — your
call — but the plan's contract is the authority, and every slice lands only
with the full gate battery green.

Its direction so far:
- `registry.pl:291` flipped to `typed([text, text], list(text))`, rendering
  `split_list_intern`.
- `lower.pl` ~665-700: new `list_intern_sql/6`; list_id =
  `(SELECT __id FROM <entity> WHERE content = <array text>)`; an
  `Encoding = list_intern(...)` threaded through `compile_expr`.

## Remaining slices (plan sections 3.2-3.4)
2. registry + producer lowering (finish the above, fail-pre-fix test first)
3. typed spread consumer: member-rel join + EXPLAIN SEARCH count test
4. oracle parity + `fixtures/19_list_value_position.pl`

## Unchanged laws
File ownership, forbidden files, gates, and baselines are exactly the plan's
section 5. One commit per slice with gate numbers. Never chain two grade.sh
runs in one shell line.
