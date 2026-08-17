# 041a stage 2: prove expanded wrapper identity targets

Continue card `relation-identity-ir` from commit `eb4bd90d8`. Implement, test, and commit this bounded stage. Do not read plans, history, issue siblings, or architecture documents.

Read only:

- `v6/prolog/compile/test/plunit_tests.pl:371-450`
- existing `wrapper_composition`, `type_wrapper_walk`, `list_column_spelling`, and self-reference option tests in that same file
- `v6/prolog/0_generic_expand.pl`
- `v6/prolog/0_option_expand.pl`

Add focused tests that call `program_plan/2` on real expanded programs, take `RelPlans`, then call `relplan_reference_targets/2`.

Required programs and expected targets:

1. Direct: `span` type used by `finding.at` yields `[span]`.
2. List: `person` type used by `team.members: list(person)` yields `[person]` through the compiler-minted member relation. The list container itself does not appear as a relation target.
3. Option: `person` type used by `commit.reviewed_by: option(person)` yields `[person]` through the compiler-minted companion relation.
4. Negative: a keyed `person` declaration with no relation-valued consumer yields `[]`.

Use existing Prolog term syntax copied from the named nearby tests. Do not invent surface syntax. If any positive case fails, repair the expanded metadata at its existing expansion seam so the canonical target query sees the stored `ref(person)` edge. Do not special-case names in `relplan_reference_targets/2`.

Run only the new PlUnit unit plus `git diff --check`. Commit with subject `dl6: retain wrapped relation identity targets` and trailer `Refs-Issue: @relation-identity-ir`.
