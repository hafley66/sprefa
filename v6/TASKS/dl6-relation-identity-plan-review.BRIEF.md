# DL6 relation identity plan review

Read these files completely:

- `/Users/chrishafley/projects/sprefa/v6/plans/2026-08-17-relation-value-identity-access.md`
- `v6/prolog/0_type_plane.pl`
- `v6/prolog/0_option_expand.pl`
- relevant list/ref lowering sections in `v6/prolog/lower.pl`
- `v6/prolog/conformance/fixtures/10_list_elements.pl`
- `v6/prolog/conformance/fixtures/19_list_value_position.pl`

Review only. Do not edit files and do not commit.

Check that the plan now consistently defines:

1. Database-local integer `Id<T>` and finite portable `Key<T>`.
2. `State<T>` separately from explicit followed `Expansion<T,P>`.
3. Monotone/tombstoned key-to-ID mapping within a database epoch.
4. Default all-column keys, explicit subset keys, and cyclic-key refusal.
5. Option relation storage as absence/presence with `option(Id<T>)`.
6. List container identity separately from `Stored<T>` member identity.
7. Identity-only access without a target join and followed access with one join.
8. Owner deletion, target replacement, dangling references, restart, and
   delete/reinsert timelines.
9. Current implementation gaps clearly labeled as future work.
10. No remaining contradictory `Value<T>`, `Id<option<T>>`, automatic reference
    accounting, or universal key claims.

Return exact file line numbers for every remaining contradiction or missing
decision. If no contradiction remains, state that and list the implementation
prerequisites in dependency order. Keep the report under 900 words.
