# Experiment G1 result

## Outcome

Completed the SeaORM port in this worktree and crate only.

- `src/temporal.rs`: 232 added lines. Defines `TemporalStore`, the append-only
  schema, the five-statement JSON-batched commit, Rust-side digest, and five
  unit tests.
- `src/lib.rs`: 1 added line, `pub mod temporal;`.
- `EXPERIMENT-G1-RESULT.md`: this result record.

The commit uses one transaction and exactly five counted SQL statements for a
non-empty delta batch: clear `d`, fold JSON into `d`, insert new live rows,
Form-A weight update, and close touched retractions.

## xorhash decision

`xorhash` remains unregistered. SeaORM/sqlx exposes no needed custom aggregate
registration path. `TemporalStore::digest` reads the live keys and XORs the
same splitmix calculation as the source in Rust. The helper is used only for
verification.

## Validation

Command:

```text
cd v6/sprefa-store
cargo test --release --lib temporal 2>&1 | grep -E "test result|FAILED|error\["
```

Output:

```text
test result: ok. 5 passed; 0 failed; 0 ignored; 0 measured; 9 filtered out; finished in 0.01s
```

Command:

```text
cd v6/sprefa-store
cargo build --release 2>&1 | grep -E "^error|^warning: unused|Finished"
```

Output:

```text
    Building [=======================> ] 183/185: sea-orm, sprefa-store
    Building [=======================> ] 184/185: sprefa-store
    Finished `release` profile [optimized] target(s) in 21.66s
```

## Snags and open questions

The process-global statement counter is shared by parallel unit tests. The
temporal test module serializes its own tests around counter use so the
constant-five assertion measures only its commit.

The repository pre-commit hook runs `dl --check`; `dl` is unavailable on this
environment's PATH, so the normal commit attempt exited before creating a
commit. The implementation, result document, and validation output are staged.
