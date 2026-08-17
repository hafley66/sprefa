# 041a compiler identity target fact

Implement and commit this card. Do not perform repository orientation.

Read only:

- `/Users/chrishafley/projects/sprefa-v6/issues/relation-identity-ir/item.md`
- `v6/prolog/0_type_plane.pl:375-415`
- `v6/prolog/lower.pl:450-470`
- `v6/prolog/lower.pl:3750-3772`
- `v6/prolog/0_generic_expand.pl`
- `v6/prolog/0_option_expand.pl`
- `v6/prolog/0_enum_expand.pl`

Existing mechanisms include `relation_reference_target/5`, `reference_target_ref/2`, and `bind_reference_target_identity/6`. Consolidate their duplicated inference behind one canonical compiler predicate or fact derived from expanded relation-valued type edges. Do not build a second identity mechanism.

The canonical result must cover direct relation columns, minted `list(Relation)` member columns, `option(Relation)` companion columns, relation-valued enum payload columns, and imported/module-qualified relations. It must exclude a keyed relation that is never used as a relation-valued target.

Add focused PlUnit coverage for those cases. Add no authored `entity`, `ref(T)`, or `embed(T)` syntax. If retained expanded metadata cannot represent enum payload or imported nominal identity, stop and report the exact missing term instead of inventing syntax.

Run focused tests and `git diff --check`. Commit with `Refs-Issue: @relation-identity-ir`. Report the commit and exact commands.
