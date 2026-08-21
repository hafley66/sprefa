# Module-path-prefixed SQLite storage names

Implement issue `module-storage-names` from
`/Users/chrishafley/projects/sprefa-v6/issues/module-storage-names/item.md`.

Read these first:

- `v6/prolog/0_rel_record.pl`
- `v6/prolog/use_resolve.pl`
- `v6/prolog/lower.pl` identifier and DDL sections
- `v6/prolog/emit_ts.pl` incremental relation plans
- `v6/prolog/emit_rust.pl` `relation_dict/5`
- `v6/sprefa-engine-rs/src/types.rs`

Preserve semantic relation names. Introduce one compiler-owned mapping from a
declaring module's normalized relative path plus exact relation name and arity
to a readable physical SQLite name. Allocate `_2`, `_3` only for
case-insensitive physical collisions, with deterministic ordering. Thread that
physical name through every SQL producer and both executable emitter plans.

Do not change authored DL6 syntax. Do not solve TS/Rust public type-name
collisions in this card. Do not use dense IDs, absolute paths, import aliases,
or traversal order.

Implement focused tests before broad generated-output updates. Run the issue's
acceptance gates. Classify unrelated baseline failures. Commit with subject
`dl6: namespace SQLite relation storage` and trailer
`Refs-Issue: @module-storage-names`.
