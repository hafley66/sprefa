# TypeSpec / SQLite export verification

Worktree: `/private/tmp/sprefa-extract-field-reports-20260908`

Implemented and verified on 2026-09-09. Changes are uncommitted in this
isolated worktree. The installed `extract` executable and the other session's
shared-checkout SQLite IVM implementation were not changed by this feature.

## Delivered

- `just gen` emits SQLite DDL, the loader's column catalog, and Rust storage
  structs from TypeSpec.
- 61 fact tables: all 60 existing `FlatFact` variants plus pattern captures.
  The catalog contains 577 columns, including export coordinates, 64 nested
  span columns and 5 JSON payload/array columns.
- Shared span declarations, family/mode/method/coverage vocabularies, and the
  TSI argument union live in TypeSpec. Rust field/variant/enum drift is tested.
- `extract --sqlite PATH ...` writes a new database and prints executable
  table/schema/query commands. Existing files are refused, including publication
  races. Failed exports do not publish a partial database.
- Plain per-file output can take multiple source files. Existing syntax,
  semantic, resolved, dependency, pattern and ingest routes share the SQL sink.
- JSONL output remains covered by the existing regression tests.

## Current build and tests

Built executable:

```text
/private/tmp/sprefa-extract-scip-reliability-target.IuDQPc/debug/extract
```

Complete Rust suite: **910 passed, 0 failed, 16 ignored**, 172 result blocks,
exit 0. Includes the 8 new SQLite integration tests.

Command, from the worktree root:

```sh
npm_config_offline=true bash v6/tools/run-capped.sh 600 cargo test \
  --offline --manifest-path v6/sprefa-extract/Cargo.toml --features cli \
  --no-fail-fast \
  --target-dir /private/tmp/sprefa-extract-scip-reliability-target.IuDQPc \
  --color never
```

This run had permission to access macOS resource counters and the pinned Go
indexer's toolchain. The existing npm-based test used local TypeScript 5.6.3
through the ignored `v6/sprefa-extract/node_modules/typescript` symlink to
`/Users/chrishafley/projects/instant/node_modules/typescript`; no npm package
was downloaded. The symlink is local test setup, not a source change.

Full-suite log:
`/private/tmp/sprefa-extract-typespec-sqlite-complete-gate-20260909.log`

`just gen-check`: **5 passed, 0 failed**, exit 0. Verifies deterministic/current
generated output, Rust span parity, SQLite fixture execution, error handling,
and compilation/execution of all generated Rust storage structs.

Generation log:
`/private/tmp/sprefa-extract-typespec-schema-final-check-20260909.log`

CI coverage adds the SQLite test target and generation checks. No existing
test coverage was removed. No CI workflow file was changed.

## Actual database

Extract read its own `src/wire.rs` and `src/tsi/types.rs` into:

```text
/private/tmp/sprefa-extract-sqlite-demo.JvZB0B/extract.db
```

15,159 total rows. Node counts: call 73, cst 5,200, df 1,482, type 27.
SQLite `integrity_check` returned `ok`. The printed table/schema/query commands
were executed successfully, including a separate destination containing spaces
and an apostrophe.

## Boundaries

This delivers new-file export. Append/update policies, migrations, IVM
installation, and replacement of existing extractor domain structs remain
outside this change. Source/kernel algorithms, soopy identity contracts,
and the other session's plugin remain unchanged. See `README.md` for NULL,
structured-span and exact unsigned-64-bit storage rules.

Only the task's rebuildable incremental compilation cache was removed during
verification (5.0 GB); the executable, test binaries and source were retained.
