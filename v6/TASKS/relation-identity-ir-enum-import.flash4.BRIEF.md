# 041a stage 3: enum, import, and metadata-loss coverage

Continue card `relation-identity-ir` from commit `ad3e1f10c`. Implement, test, and commit this bounded final stage. Do not inspect plans, history, issue siblings, or unrelated runtime code.

Read only:

- the `relplan_reference_targets` and `wrapped_relplan_reference_targets` units in `v6/prolog/compile/test/plunit_tests.pl`
- the nearest existing enum-expansion tests in that file
- the nearest existing `mount_door` imported-type tests in that file
- `v6/prolog/0_enum_expand.pl`
- `v6/prolog/use_resolve.pl`

Add focused compiler tests proving:

1. A relation-valued enum payload retains its nominal target in the expanded `RelPlans`, and `relplan_reference_targets/2` returns that relation name.
2. A module-qualified imported relation used as a column type retains its resolved nominal target in `RelPlans`.
3. A keyed enum, imported relation, or ordinary relation unused as a relation-valued column does not become a target.

If either positive path loses its target metadata, repair only the existing enum/import expansion seam. Do not add authored `ref(T)`, `entity`, or `embed(T)` syntax.

Add one focused named refusal only if the expansion accepts a relation-valued wrapper while erasing the target so completely that the compiler cannot reconstruct it. If existing expansion retains enough metadata after the repair, document that the refusal is unreachable by construction and do not invent an error.

Run only the new unit, the nearby enum/import units, and `git diff --check`. Commit with subject `dl6: retain enum and imported identity targets` and trailer `Refs-Issue: @relation-identity-ir`.
