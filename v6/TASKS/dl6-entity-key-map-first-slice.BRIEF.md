# DL6 entity key-to-ID first slice

Read completely:

- `/Users/chrishafley/projects/sprefa/v6/plans/2026-08-17-relation-value-identity-access.md`
- `v6/prolog/0_type_plane.pl`
- key/refcount/arrival/edge DDL and delta paths in `v6/prolog/lower.pl`
- TypeScript and Rust struct/reference ingress implementations
- relevant key, replacement, retraction, and restart tests

Establish the smallest implementation path for database-local integer entity
identity:

```text
row_key<T>(State<T>) -> Key<T>
row_id<T>(Key<T>) -> Id<T>
row_state<T>(Id<T>) -> at most one live State<T>
```

Required laws:

- integer scope is `(database epoch, nominal relation type)`
- key-to-ID mapping survives state deletion during the epoch
- IDs are not reused for another key during the epoch
- same-key simultaneous unequal state is a deterministic conflict
- later-frontier replacement is atomic `-old,+new` and retains the ID
- logs, occurrences, level/refcount relations, and keyed edges keep their
  existing semantics unless explicitly declared as entity relations

Work mode:

1. Trace actual current call paths and schemas with file/line evidence.
2. Choose the narrow entity/arrival relation scope already supported by current
   metadata. Do not universalize keys across every relation class.
3. If a persistent identity-map table and transaction ordering can be added
   without changing unrelated relation semantics, implement it with focused
   delete/reinsert, replacement, conflict, restart, and ID-nonreuse tests.
4. If the current IR cannot distinguish entity relations, stop before edits and
   return the exact missing discriminator and the minimal type/IR addition.

Do not use content hashes or strings as stored IDs. Do not alter list or option
storage. Do not loosen existing failures. Run focused gates and `git diff
--check`. Commit one coherent change with `Refs-Issue: @entity-key-map` only if
the laws are proven.
