# Typed SQLite storage checkpoint

Worktree branch: `feature/extract-tsp-rusqlite-20260909`.
TypeSpec toolchain: `hafley-tsp` local `main`, commit `834b4c5`.

## Acceptance

- TypeSpec generates strict input models, nested-span bindings, JSON codecs,
  exact unsigned-integer storage and typed insert methods.
- Application code supplies source coordinates and the transaction.
- SQLite export batches rows, flushes the final partial batch, and retains
  duplicate facts under integer emission ordinals.
- Raw facts produced during `fast` and `--resolve` reach SQLite alongside
  resolved facts. Existing JSONL output remains unchanged.
- New-file export still publishes only after successful commit and refuses
  existing destinations. Cache refresh, migrations and IVM are separate work.

## Driver constraint

Extract uses rusqlite 0.40 with libsqlite3-sys 0.38.2. SQLx 0.9.0 declares
`libsqlite3-sys >=0.30.1, <0.38.0`, so adding that driver would require
aligning the native SQLite dependency stack. This increment retains rusqlite.
The existing TypeSpec SQLx package is independent of extract's runtime.

## Dogfood: effect calls and loops

An existing extract binary scanned the in-progress SQLite module, project
resolver and TypeSpec writer emitter. The result was 54,701 rows in
`/private/tmp/extract-nplus1.qiMHVE/storage.db`. This is a point-in-time scan;
rerun after edits before using its byte offsets.

```sql
SELECT DISTINCT s._input_path, s.callee, s.span__start, n.depth
FROM site AS s
JOIN df_nest AS n
  ON n._input_path = s._input_path
 AND s.span__start >= n.call__start
 AND s.span__end <= n.call__end
WHERE s.callee IN
  ('read_many', 'read', 'prepare_cached', 'execute', 'insert', 'insert_all')
ORDER BY s._input_path, s.span__start;
```

The query identifies review candidates. A call named `insert` can target an
in-memory map; a closure passed to `map` can perform repeated I/O without a
syntactic loop row. Inspect the receiver, callers and effect implementation.

Source review identified two introduced costs to remove: formatting source
hashes per fact and hashing file bytes again for metadata. It also identified
existing reader work outside this storage increment: `open_files` reads bytes
to construct entries, followed by another read through `read_many`; the
single-blob adapter calls `read_many` with one request. These paths expose
current-worktree freshness behavior, so changing their caching needs a
separate contract review.

After integration, extract scanned the generated Rust writer and the CLI
storage module into 4,257 rows in
`/private/tmp/extract-nplus1.qiMHVE/generated-writer.db`. Queries reported
61 static `prepare_cached` sites, 61 `execute` sites, one `insert_all` call
site and one row-insert loop. The writer performs one SQLite step per row
inside the caller's transaction. It does not generate multi-row `VALUES`
statements or per-row commits. The CLI statement cache covers all 61 tables.
The resulting database passed `PRAGMA integrity_check`.

The CLI storage module changed from 335 to 215 handwritten lines.
Generated bindings replace the runtime catalog walker.
Source hash formatting occurs on source changes, and file metadata reuses
the reader's existing hash. The legacy reader now also computes line-count
metadata, but does not flatten raw facts unless the raw sink is requested.

## Verification

- Complete extract Rust suite: 913 passed, 0 failed, 16 ignored across 173
  result blocks, exit 0. Log:
  `/private/tmp/extract-typed-full-gate-20260909.log`.
- SQLite integration: 9 passed, including a 301-row batch fixture, source
  boundaries, final partial-batch flush, duplicate preservation, unsigned
  limits, all 61 single-integer primary keys and zero secondary indexes.
- Raw retention: exact fact-and-coordinate parity for two paths containing
  identical bytes, unchanged resolved answers, and immediate sink-error stop.
- Existing DDL and catalog are byte-identical to the prior commit.
- `just gen-check`: 5 passed, 0 failed, including deterministic generation and
  compilation/execution of 3 generated-Rust tests. Log:
  `/private/tmp/extract-typed-gen-check-20260909.log`.
- Final `@hafley/typespec-rusqlite` package suite: 9 passed, 0 failed. The
  added emitter test uses different model, discriminant and source names.
  Log: `/private/tmp/tsp-typed-package-tests-20260909.log`.

CI coverage adds the batch/index test and two raw-retention tests, and extends
existing generated-Rust and package tests. No CI coverage is removed.
