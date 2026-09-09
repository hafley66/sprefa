# Generated rusqlite writer integration, 2026-09-09

Worktree: `/Users/chrishafley/projects/sprefa/.recovery/extract-rusqlite-20260909`.
Branch: `feature/extract-tsp-rusqlite-20260909`.
Base: recovered commit `23cac35c8`.
Toolchain: `hafley-tsp` local `main`, commit `fb9a198`.

## Implemented path

```text
1_facts.tsp
  -> nested-field storage projection
  -> @hafley/typespec-sql: DDL
  -> @hafley/typespec-rusqlite: 7_writers_auto.rs
  -> extract validates wire values using the generated catalog
  -> generated insert_values(&connection, table, &values)
  -> cached parameterized INSERT inside the existing export transaction
```

The public `emitRusqliteValueWriters` API generates static statements for
ordinary entity models. It accepts caller-validated `rusqlite::types::Value`
slices and adds no transaction or savepoint. Extract supplies its existing
typed conversion for JSON TEXT, nullable fields and exact unsigned integers.

The generated dispatcher covers all 61 tables. Existing DDL, catalog and
Rust row declarations are byte-identical to the recovered commit. The runtime
no longer constructs INSERT SQL. No compiler/extraction semantics, config,
cache, interning policy, or IVM behavior changed.

## Verification

- Complete extract Rust suite: 910 passed, 0 failed, 16 ignored across 172
  result blocks, exit 0. No test was skipped beyond its existing ignore marker.
  Log: `/private/tmp/extract-rusqlite-final-gate-20260909.log`.
- `just gen`: 8 generated artifacts.
- `just gen-check`: 5 passed, 0 failed. Includes compiled generated Rust,
  execution through a caller-owned transaction, rollback, unknown table and
  incorrect parameter-count errors.
- `pnpm --filter @hafley/typespec-rusqlite build` succeeded.
- `pnpm --filter @hafley/typespec-rusqlite test`: 8 passed, 0 failed, including
  existing generated typed/interned writer execution and new positional
  emission/unsupported-dialect coverage.
- SQLite integration coverage now includes signed-64 maximum, its unsigned
  successor, unsigned-64 maximum and nullable unsigned timestamps.

Self-extraction read `v6/sprefa-extract/src/wire.rs` and
`v6/sprefa-extract/src/tsi/types.rs` using both the recovered and new binaries:

```text
/private/tmp/extract-rusqlite-smoke.zYcTlY/recovered.db
/private/tmp/extract-rusqlite-smoke.zYcTlY/generated.db
```

Both contain 15,159 rows. Complete `sqlite3 .dump` output is byte-identical;
the new database's `PRAGMA integrity_check` returns `ok`.

CI coverage extends existing generator and SQLite tests and adds a rusqlite
package emission check. No test or CI workflow was removed or disabled.

## Reproduction

From this worktree, with the sibling TypeSpec packages built:

```sh
just gen
just gen-check
npm_config_offline=true CARGO_TERM_PROGRESS_WHEN=never \
  CARGO_TARGET_DIR=/private/tmp/extract-recovered-cargo.8prmG2 \
  bash v6/tools/run-capped.sh 600 cargo test --offline \
  --manifest-path v6/sprefa-extract/Cargo.toml --features cli --no-fail-fast
```

Full-suite tests require macOS resource-counter access and access to the
existing pinned Go SCIP indexer's toolchain. The local ignored TypeScript
dependency symlink is retained for the existing npm-based tests.
