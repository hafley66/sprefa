# Typed SQLite storage checkpoint

Worktree branch: `feature/extract-tsp-rusqlite-20260909`.
TypeSpec toolchain: `hafley-tsp` local `main`, commit `1af0853`.

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

At the previous checkpoint, extract scanned the generated Rust writer and the CLI
storage module into 4,257 rows in
`/private/tmp/extract-nplus1.qiMHVE/generated-writer.db`. Queries reported
61 static `prepare_cached` sites, 61 `execute` sites, one `insert_all` call
site and one row-insert loop. That checkpoint performed one SQLite step per row
inside the caller's transaction. The current generator groups typed references
by table and emits multi-row `VALUES` statements bounded by runtime variable
and SQL-length limits. The CLI statement cache has 61 entries; different chunk
sizes can occupy distinct entries.
The resulting database passed `PRAGMA integrity_check`.

Generated bindings replace the runtime catalog walker.
Source hash formatting occurs on source changes, and file metadata reuses
the reader's existing hash. The legacy reader now also computes line-count
metadata, but does not flatten raw facts unless the raw sink is requested.

## Runtime-bounded batching

The generated writer preserves original ordinals after grouping and preflights
ordinal overflow and all present-table capacities before writes. It inserts
each table chunk with one `raw_execute`, leaving rollback to its caller.
The CLI buffers against a runtime-derived row ceiling and an 8 MiB serialized
input budget. Oversized individual facts flush alone. Source changes, explicit
flushes and completion flush the remaining rows. Serialized bytes are an
accounting bound, not an exact heap-size measurement.

The generated Rust test lowers the variable limit to 10. Five mixed records
produce four traced INSERT executions: protocol ordinals 10,12,13 and size_skip
ordinals 11,14. It also tests exact one-row and two-row SQL-length capacities,
below-one-row rejection, all-present-table preflight without partial writes,
later-chunk failure followed by caller rollback, and ordinal overflow.

The CLI accounting test uses small facts with declared encoded sizes. Pending
rows/bytes and stored-row counts prove the immediate oversized flush, a flush
when two individually fitting facts exceed the combined budget, and final flush.
The integration test separately writes and reads an actual oversized JSON value.

## Current focused verification

- Complete extract suite: 915 passed, 0 failed, 16 ignored across 173 result
  blocks, exit 0. `/private/tmp/extract-takeover-full-gate-20260909.log`.
- `just gen-check`: 5 passed, including 3 generated Rust tests.
  `/private/tmp/extract-gen-check-20260909.log`.
- TypeSpec rusqlite package: 9 passed.
  `/private/tmp/rusqlite-package-test-20260909.log`.
- Extract SQLite integration target: 10 passed.
  `/private/tmp/extract-sqlite-integration-20260909.log`.
- Explicit CLI byte-accounting unit test: 1 passed.
  `/private/tmp/extract-sqlite-bin-unit-rerun-20260909.log`.

CI coverage adds byte-buffer accounting assertions and extends generated SQL
execution-count, limit-boundary, preflight and rollback assertions. No coverage
is removed by this increment.
