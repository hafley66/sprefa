# Extract source recovery, 2026-09-09

Recovered branch: `recovery/extract-schema-20260909`.
Base commit: `8342be4f7d22008eb879f72765dd0a3ad4a37f7c`.
Persistent worktree: `/Users/chrishafley/projects/sprefa/.recovery/extract-20260909`.

## Provenance

Boop transcript queries located original session
`01a08312-7969-7f71-9a61-9f9a21872375`, whose complete transcript is:

```text
/Users/chrishafley/.codex/sessions/2026/09/08/rollout-2026-09-08T18-10-21-01a08312-7969-7f71-9a61-9f9a21872375.jsonl
```

Recovery replayed 22 successful source patches after the base commit, in
transcript order. Patch header paths were relocated; source bodies were
preserved. An originally failed patch at transcript line 2751 was excluded;
its successful correction was replayed. The untracked task brief was recovered
separately from patches at transcript lines 184 and 222.

The resulting source-file list matches the recorded pre-deletion status in
call `call_w6Dc21w0kybrWCrN9BDIWwqr` at transcript line 3091. Generated artifacts
were recreated with `just gen`. Cargo restored the direct `tempfile` dependency
edge without changing locked versions. Rust formatting was run before commit.
This receipt is new; `3_verification.md` preserves the original historical record.

Local patch payloads, SHA-256 hashes, call IDs and replay results are retained in:

```text
/Users/chrishafley/projects/sprefa/.recovery/extract-20260909-evidence/
```

## Recovery verification

- `just gen`: seven artifacts generated.
- `just gen-check`: 5 passed, 0 failed, including generated Rust compilation.
- SQLite CLI integration target: 8 passed, 0 failed, 0 ignored.
- Generated catalog: 61 tables, 577 columns, 64 nested columns, 5 JSON columns.

The focused Rust command was run from the recovered worktree root:

```sh
npm_config_offline=true cargo test --offline \
  --manifest-path v6/sprefa-extract/Cargo.toml --features cli --test 0_sqlite \
  --target-dir /private/tmp/extract-recovered-cargo.8prmG2 --color never
```

The restored coverage checks Rust/TypeSpec variant and field parity, every
table's row ingestion, JSONL/SQL parity across extraction modes, file/content
coordinates, ingest atomicity, malformed input, existing paths, publication
races, duplicates and unsigned integer limits. The full Rust suite was not
rerun during recovery. No CI workflow was changed or existing test removed.

The sibling dependency link `.recovery/hafley-rs` points to the existing
`/Users/chrishafley/projects/hafley-rs` checkout. Generation uses the existing
sibling `hafley-tsp` toolchain; no package download was required.

## Scope

This restores the recorded TypeSpec schemas, generators, generated outputs,
SQLite exporter, CLI integration, tests and task brief. New rusqlite-emitter
adoption, config/cache changes and IVM integration are separate work. No merge
into the active shared checkout was performed. The deletion's cause has not
been established by this recovery.
