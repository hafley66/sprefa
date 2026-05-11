# V4 CursorValue And WhereBytes Plan

Date: 2026-05-09

This plan records the refactor started after the Linux ast-grep performance regression. The goal is to keep source bytes addressable by durable byte ranges while preventing whole file bodies from crossing queue boundaries by default.

## Current Commits

- `0daddc7 refactor: add where-bytes store names`
- `f02cafa refactor: encode cursor value handles`
- `b9a193d docs: add cursor value refactor plan`
- `758e656 feat: make ast consume source paths`
- `3bded1d docs: mark source aware ast complete`
- `fc516ee feat: make v4 bench source aware`
- `36401a8 perf: tighten source aware ast read path`

These commits were pushed from local `main` to `origin/master` on 2026-05-09.

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
CursorTerm       named value attached to a cursor; `&` is the focal term
CursorValue      small typed payload handle stored on a CursorTerm
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

- `Cursor` stores `Vec<CursorTerm>` plus a process-local weak store handle.
- The focal cursor value is a real term named `&`.
- `Cursor` derefs to the focal term, so existing `cursor.value`, `cursor.at`,
  `cursor.value_id`, and `cursor.cursor_value` field access delegates to `&`.
- `Cursor::get("&")` and `Cursor::get("&.value")` both read the focal term value.
- `Cursor::set("&", value)` and `Cursor::set("&.value", value)` both write the focal term value.
- `Row::fields()` exposes the full term list, with default cursors placing `&` first.
- Queue codec encodes the term list directly, including each term's `CursorValue`, `StringId`, and `Ref`.
- Full migration should move hot paths off the string display lane for source bytes.

Current uniformity boundary:

```text
public cursor API:
  &              focal value
  &.value        focal value alias
  NAME           named cursor term
  NAME.value     named cursor term alias

current storage:
  &              CursorTerm { name: "&", value, cursor_value, value_id, at }
  NAME           CursorTerm { name: "NAME", value, cursor_value, value_id, at }
```

Normal `Cursor::set` cannot create a named term that shadows `&`. If a manually
constructed duplicate `&` term exists later in the term list, `Cursor::get("&")`
uses the first focal term.

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
- Default language semantics still enumerate files under the root. Bench code can
  attach an explicit source-side extension filter so perf comparisons can avoid
  dispatching non-candidate files.

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

## Completed Slice

Committed in `758e656`:

```text
fs > ast
```

matches a Rust function without `read`, and the emitted match cursor does not contain the whole file body.

Test command:

```bash
RUSTC_WRAPPER= cargo test --manifest-path v4/Cargo.toml --test source_aware_ast_smoke -- --nocapture
```

Status: passed on 2026-05-09.

## Implementation Steps

1. Finish source-aware `AstNmComponent`.
   - If `FS` term points to a file and `cursor.value` is the path, read that file inside `ast`.
   - Intern file content and path through `SprfStore`.
   - Preserve repo/rev from upstream `WhereBytes` when present.
   - Emit match cursor with `CursorValue::WhereBytes`.
   - Status: committed in `758e656`.

2. Make `fs` stamp a small cursor value.
   - During migration, keep `cursor.value = path`.
   - Set `cursor_value = CursorValue::String(path_string_id)` at minimum.
   - Later, use a full-file `WhereBytesId` if source length/file id is known cheaply.
   - Status: minimum path-string handle committed in `758e656`.

3. Keep `read` as explicit materialization.
   - It can still set legacy `cursor.value` to body text.
   - Also set `cursor_value = CursorValue::String(value_id)` or `Blob`.

4. Add telemetry intentionally.
   - rows in/out per stage
   - queue encoded bytes later
   - source bytes resolved
   - source bytes materialized as UTF-8
   - prefilter skipped files
   - parse count
   - wall time per stage
   - Status: `AstTelemetry` reports input rows, source read rows, source bytes,
     source UTF-8 rows, source UTF-8 bytes, source errors, legacy rows,
     fixed-prefilter skips, parses, and matches.

5. Clean up old names after behavior is green.
   - Replace docs and comments that say `_refs`.
   - Migrate public store methods from `intern_ref`/`coord_of` to `intern_where_bytes`/`where_bytes_of`.
   - Keep aliases only as long as tests need them.

## Perf Gate

Use the linux fixture:

```text
v3/tests/smoke/.fixtures/linux
```

Run commands:

```bash
RUSTC_WRAPPER= cargo build --manifest-path v4/Cargo.toml --release --bin v4-bench
just v4-bench-linux
just v4-bench-linux-read
just v4-bench-linux-sprf
just v3-bench-linux
```

Just recipe parameter overrides are positional:

```bash
just v4-bench-linux-quick
just v4-bench-linux "v3/tests/smoke/.fixtures/linux" 'printk($$$)' 8 1 4096
just v4-bench-linux "v3/tests/smoke/.fixtures/linux" 'printk($$$)' 6 3 4096
```

Do not run benchmark recipes in parallel. CPU contention makes wall time useless
and can make idle-thread experiments look worse than the default.

Target query:

```text
printk($$$)
```

The system should get back near the V3/direct-scan shape. The exact threshold should be pinned after telemetry is no longer bench-local.

2026-05-09 current-machine results:

```text
V3 batch, workers=8, trials=3
  files=63482
  matches=16627
  p50=4.832s

V4 source-aware fs > ast, workers=8, trials=3
  files after ext filter=63482
  matches=16627
  p50=5.099s
  read_rows=0
  peak RSS max across trials=220 MB
```

This is back near the V3 shape but still above the older sub-4s target. The remaining measured cost is in the `ast` stage, not in explicit `read`, because the default benchmark now keeps source body materialization inside the matcher.

Telemetry added after that p50 run exposed the matcher-internal shape:

```text
V4 source-aware fs > ast, source-side ext filter, workers=8, trials=1
  files after ext filter=63482
  matches=16627
  wall=5.361s after release rebuild
  fs_seen=93299
  fs_ext_skipped=29817
  rendered=143592
  emitted=143591
  rss_peak_MB=158
  source_reads=63482
  source_MB=1342.2
  source_utf8=4495
  source_utf8_MB=109.3
  source_errors=0
  legacy_rows=0
  prefilter_skips=58987
  parses=4495
```

The current AST fast path now has the same coarse cardinality as the V3
shape: read every candidate source file, skip most files through the fixed
string prefilter, parse only the remaining source set, emit match cursors
without explicit `read` queue rows.

After the raw-byte fixed prefilter, skipped files do not become UTF-8 strings
and do not get interned as source bodies by `ast`; only the `4495` parse
candidates are materialized for ast-grep.

Before the source-side extension filter, the bench emitted `93299` file cursors
and then dropped `29817` through a downstream `ext_filter` component. The new
bench path keeps default `fs` semantics unchanged but lets the benchmark match
V3's pre-dispatch corpus filtering.

2026-05-10 current-machine check after release rebuild:

```text
V3 existing release binary, batch mode, workers=8, trials=3
  files=63482
  matches=16627
  p50=3.876s

V4 source-aware v4-bench, workers=8, trials=3
  files after ext filter=63482
  matches=16627
  p50=4.267s
  read_rows=0
  source_reads=63482
  source_MB=1342.2
  source_utf8=4495
  source_utf8_MB=109.3
  prefilter_skips=58987
  parses=4495
  peak RSS around 200 MB

V4 explicit read path, workers=8, trials=1
  read_rows=63482
  read_MB=1342.2
  peak RSS around 1.7 GB
```

Current bottleneck evidence:

```text
batch size:
  v4-bench uses batch=4096 and expand batch_cap=max(batch, 65536)
  app/sprefa-run uses batch_cap=4096 for normal runs

N+1 / materialization:
  default v4-bench has read_rows=0
  skipped files do not become UTF-8 strings
  explicit read is the known bad memory path

CPU contention / idle threads:
  workers=8 is best on this machine in quick sweeps
  workers=6 p50=4.736s
  workers=10 p50=4.694s
```

The `.sprf` bench is currently slower than `v4-bench` because it expresses:

```sprf
fs > glob`**/*.{c,h}` > ast(:c)`printk($$$)` > fact(:hits, FS, LO, HI);
```

That queues all `93299` file paths before `glob`. `v4-bench` pushes the c/h
filter into `FsComponent`, so only `63482` candidate paths become queue rows.
The next perf slice is either a lift/fusion from `fs > glob` into `FsComponent`
or an explicit source-filter op that keeps the normal language path honest.

## Dirty Worktree Notes

Known unrelated dirty files at time of note:

```text
.beads/issues.jsonl
chat_log/LATEST.md
.agents/
.codex/
chat_log/20260507.2.v4-rule-engine-respec-and-memory-audit.md
```

Bench telemetry file:

```text
v4/src/bin/v4_bench.rs
```

That file now contains the intended benchmark telemetry surface for this
slice. It is still bench-local; runtime-wide telemetry and queue encoded-byte
telemetry remain later work.
