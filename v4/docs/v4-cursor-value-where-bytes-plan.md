# V4 CursorValue And WhereBytes Plan

Date: 2026-05-09

This plan records the refactor started after the Linux ast-grep performance regression. The goal is to keep source bytes addressable by durable byte ranges while preventing whole file bodies from crossing queue boundaries by default.

## Current Commits

- `0daddc7 refactor: add where-bytes store names`
- `f02cafa refactor: encode cursor value handles`

These commits are local on `main` at the time this note was written.

## Problem

The V4 linux ast-grep benchmark regressed because the split pipeline shape:

```text
fs > read > ast
```

materialized every file body into `cursor.value` before `ast` could apply ast-grep's fixed-string prefilter. V3's fast path read, prefiltered, parsed, and matched inside one parallel closure, so most files never became queued source-body cursors.

The desired shape:

```text
fs > ast
```

where `fs` emits source handles and `ast` resolves bytes privately.

## Core Vocabulary

Target names:

```text
Coord        -> WhereBytes
Ref          -> WhereBytesId
_refs        -> _where_bytes
Term         -> CursorTerm
```

Meaning:

```text
WhereBytes       repo/rev/file/lo/hi byte span
WhereBytesId     persisted id for a WhereBytes row
StringId         interned string id in _strings
CursorTerm       named value attached to a cursor
CursorValue      small typed focal payload handle
```

Current code still keeps `Coord`, `Ref`, and `Term` compatibility names while call sites migrate.

## CursorValue Target

Current committed enum:

```rust
pub enum CursorValue {
    Null,
    Bool(bool),
    Int(i64),
    Float(u64),
    String(StringId),
    WhereBytes(WhereBytesId),
    Blob(BlobId),
}
```

`Float(u64)` stores deterministic `f64::to_bits()` form.

Migration state:

- `Cursor.cursor_value` exists.
- `Cursor.value: Arc<str>` still exists as the legacy display/body lane.
- Queue codec encodes `cursor_value` as a compact tag plus payload.
- Full migration should move hot paths off `Cursor.value` for source bytes.

## Store Target

Committed store changes:

```text
_strings(id, content, norm)
_where_bytes(id, file_id, lo, hi, repo, rev)
_string_observations(id, string_id, role, where_bytes_id, context_kind, context_id)
```

`norm` is one algorithm:

```text
lowercase + keep only alphanumeric chars
ApiService/api_service/api-service -> apiserviceapiserviceapiservice
```

`_string_observations` is intentionally one role table. The table answers "where was this string seen, and in what role?" without creating N role-specific tables first.

## Source Byte Rule

Queue rows should carry small handles:

```text
CursorValue::WhereBytes(WhereBytesId)
CursorValue::String(StringId)
CursorValue::Blob(BlobId)
```

Queue rows should not carry whole source bytes unless a program explicitly asks for materialization through `read`.

## Operator Semantics

`fs`

- Emits one cursor per file.
- Legacy surface: `cursor.value` remains the path string during migration.
- Target surface: `cursor_value` should identify source location or path string handle.
- Does not read file body.

`read`

- Explicit materialization boundary.
- Takes a source/path cursor and loads source bytes.
- May produce `CursorValue::String(StringId)` for valid text or `CursorValue::Blob(BlobId)` when blob storage exists.
- It is allowed to be slower because the user requested bytes.

`ast`

- Should accept source handles directly.
- Resolves bytes privately through store/source resolver.
- Applies ast-grep fixed-string prefilter before parse.
- Emits match cursors with `at = WhereBytesId(match range)`.
- Match cursor should not carry the whole file body in `cursor.value`.

Legacy fallback:

- `read > ast` keeps working while tests and examples migrate.
- If input has no source handle, `ast` may still consume legacy `cursor.value` text.

## In-Progress Slice

Uncommitted at this handoff:

- `v4/tests/source_aware_ast_smoke.rs`
- `v4/src/v2_ops.rs`

Target test:

```text
fs > ast
```

should match a Rust function without `read`, and the emitted match cursor should not contain the whole file body.

Suggested test command:

```bash
RUSTC_WRAPPER= cargo test --manifest-path v4/Cargo.toml --test source_aware_ast_smoke -- --nocapture
```

The latest known red result before implementation was:

```text
left: 0
right: 1
```

meaning `ast` emitted no matches from `fs` without `read`.

## Implementation Steps

1. Finish source-aware `AstNmComponent`.
   - If `FS` term points to a file and `cursor.value` is the path, read that file inside `ast`.
   - Intern file content and path through `SprfStore`.
   - Preserve repo/rev from upstream `WhereBytes` when present.
   - Emit match cursor with `CursorValue::WhereBytes`.

2. Make `fs` stamp a small cursor value.
   - During migration, keep `cursor.value = path`.
   - Set `cursor_value = CursorValue::String(path_string_id)` at minimum.
   - Later, use a full-file `WhereBytesId` if source length/file id is known cheaply.

3. Keep `read` as explicit materialization.
   - It can still set legacy `cursor.value` to body text.
   - Also set `cursor_value = CursorValue::String(value_id)` or `Blob`.

4. Add telemetry intentionally.
   - rows in/out per stage
   - queue encoded bytes
   - source bytes resolved
   - prefilter skipped files
   - parse count
   - wall time per stage

5. Clean up old names after behavior is green.
   - Replace docs and comments that say `_refs`.
   - Migrate public store methods from `intern_ref`/`coord_of` to `intern_where_bytes`/`where_bytes_of`.
   - Keep aliases only as long as tests need them.

## Perf Gate

Use the linux fixture:

```text
v3/tests/smoke/.fixtures/linux
```

Target query:

```text
printk($$$)
```

The system should get back near the V3/direct-scan shape. The exact threshold should be pinned after telemetry is no longer bench-local.

## Dirty Worktree Notes

Known unrelated dirty files at time of note:

```text
.beads/issues.jsonl
chat_log/LATEST.md
.agents/
.codex/
chat_log/20260507.2.v4-rule-engine-respec-and-memory-audit.md
```

Known related dirty files at time of note:

```text
v4/src/v2_ops.rs
v4/tests/source_aware_ast_smoke.rs
```

Earlier bench experiment file:

```text
v4/src/bin/v4_bench.rs
```

That file contains telemetry experiments and should be reviewed before committing as product code.
