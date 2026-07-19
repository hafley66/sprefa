# Auto-refactor: `use`-path rewrite via byte-span edits

> **Audit 2026-07-18 (branch auto-refactor)**: every numbered step below LANDED
> pre-v5-lift; per-step receipts inline. File paths in the body are the old
> `v5/src` layout; current locations in the receipts. One follow-on landed with
> this audit: statement-level regroup when a brace leaf's rewrite exits its
> head (was a loud skip). Still loud skips, by design: `self`/`*` groups, a
> moved file's own brace-inner leaves.
>
> | Step | Status | Receipt |
> |---|---|---|
> | P0 `of_located` | LANDED 2026-05-31 | 1fdf2bd3; extended to fold `repo` too (2a471356); `src/spine.rs:192`, chokepoint `src/engine/meta.rs:1534` |
> | P1 hoist | LANDED 2026-05-31 | 1fdf2bd3; batched via `insert_module_spans` -> one `insert_spine_where_bytes` (`src/engine/extract/mod.rs:547`, full-scan `src/engine/pipeline/full_sources.rs:274`) |
> | F1 leaf spans | LANDED 2026-05-31, design revised | 1fdf2bd3; brace leaves share the outermost HEAD span in the spine (`src/graph/modgraph/rust.rs` `expand_use_leaves`), leaf granularity handled by the move sink's second pass (#17, f859585e) |
> | `ref.id` | LANDED 2026-05-31 | 1fdf2bd3; `ref(id, string, file, lo, hi)` is the reserved spine rel |
> | A driver + sink | LANDED 2026-05-31/06-01 | 09465572 + 79db9d9b + 35834e4b; `run_move`/`move_one_repo` `src/lib.rs`, sink `src/refactor.rs`, path math `src/rspath.rs`. Deviation: no `edit` db table — in-memory `refactor::Edit` batch, dry-run preview + `--fix` splice. LSP rename NOT wired (rides the parked LSP-thin-client plan) |
> | "residuals" (brace-inner leaves, physical rename + mod surgery, moved file's own imports) | LANDED 2026-06-12 | f859585e (#17); Kotlin `--move` landed later (`src/ktpath.rs`). The CLAUDE.md parked line "brace-head use rewrite, physical file move" predated #17 and was stale |
> | B DSL operator | STILL DEFERRED | correct per plan — no `rewrite_use` operator exists |

The OG v0 use case, riding ref-spine C. Rewrite Rust `use` paths by editing the byte
spans the spine already records. `ref` = import graph AND rewrite coordinate; the
"reverse refs" demo IS the refactor query.

Decisions locked (2026-05-31, after a 3-Sonnet + 1-Opus interning panel):
- **F1 = brace leaves too**: `expand_use` threads byte offsets through its recursion;
  every leaf inside `{…}` gets its own `(lo,hi)`, plus bare uses.
- **F2 = Route A now, Route B deferred** (not "A then B next"). A Rust `--move` driver
  computes rewrites and interns inline (O(1-5) strings, one `insert_rows`). Route B
  (DSL `rewrite_use` operator + rule emits `edit()`) stays UNBUILT until someone
  actually wants a DSL-authored rewrite — and then only B-batched (staging text →
  `StringId::of` → one `insert_rows`), never B-naive (per-row UDF intern = the v4 trap).

## Why the panel mattered: v4's N+1 was implementation, not interning

| Thing | v4 | v5 |
|---|---|---|
| Interning concept (content-addressed ids) | fine — DashSet-seen + 16384-buffered | fine — `StringId::of` = pure blake3, no SQL |
| Per-row writes | the bug: `SqliteFactStore::insert` 63k× unbatched | collect → `insert_rows` 256-chunks, one bump/tick |
| Giant blobs as interned string columns | the bug: multi-KB `_memo` hex → ~1M interns | doesn't exist in v5 |

v4 numbers (linux fixture): `store_insert_calls` 63,483→0, `string_intern_calls`
253,983→63,534 once batched. `intern_output_cursors` (the route-B ancestor) was already
batched — never the offender. Output-interning is safe iff it goes through
`insert_rows` and rewrite text is a normal `_strings` row.

## Prerequisites (correctness, not polish — load-bearing once edit-spans exist)

### P0 — fold `path` into `_where_bytes` row identity
`WhereBytesId::of` hashes `(string,repo,rev,file,lo,hi)`, path EXCLUDED, and `FileId` is
content-addressed. Two byte-identical files (mod.rs re-export stubs, generated shims —
exactly what a crate-move targets) with the same `use` line at the same offset collapse
to ONE `WhereBytesId`; `INSERT OR IGNORE` keeps the first `path`, drops the second.
Worse, this corrupts `retract_paths` ([engine.rs:911](../v5/src/engine.rs#L911)) both
ways under `--changed`: retract one path → stale span lingers, or deletes a span the
twin still needs.

Fix (keeps `WhereBytes` a pure coordinate): new `WhereBytesId::of_located(w, path)` that
folds `path` on top of the existing id; the single stored-id chokepoint
([engine.rs:1521](../v5/src/engine.rs#L1521)) switches to it. `push_span` and the
`WhereBytes` struct are untouched — path enters only at id-compute, from the tuple
already threaded as `(String, WhereBytes)`. Tests [spine_meta.rs:194,249,294] recompute
the id; pass `"src/a.rs"`. Retires the C5 "collapse repaired on full tick" invariant
(it only existed to paper over this). `ref(string,file,lo,hi)` query relation
unaffected — it projects coords, not ids.

### P1 — hoist `insert_spine_where_bytes` out of the per-rule/per-file loop
The `--changed` path ([engine.rs:606-616](../v5/src/engine.rs#L606)) calls
`insert_spine_where_bytes` once per matching rule per changed file. Latent per-file N+1
today; adding `use`-specifier spans makes every changed Rust file cross `N1_THRESHOLD=64`
sooner. Collect `where_rows` across the whole rule loop, call once after (mirrors the
full-scan path at [engine.rs:882](../v5/src/engine.rs#L882)).

## Feature (post-reload)

### F1 — thread byte offsets through the module resolver
- `ModuleRef` ([modgraph.rs:34](../v5/src/modgraph.rs#L34)) gains
  `span: Option<(u32,u32)>` (byte lo/hi of the rewritable path text in file content;
  `None` for paths with no clean single coordinate).
- `expand_use` ([modgraph.rs:307](../v5/src/modgraph.rs#L307)) carries a base offset
  through `rec`, emitting `(path, leaf_lo, leaf_hi)` for brace leaves AND bare uses.
  `strip_noise` preserves byte offsets, so a match start is a real file coordinate.
  Hazard: within one brace group the same identifier text can appear at different spans
  (`use a::{b, c::b}`) — fine, distinct lo/hi → distinct ids (given P0). The `edit` sink
  keys on span/`ref_id`, never on resolved-path text.
- `module_rows_for_rev` ([engine.rs:1131](../v5/src/engine.rs#L1131)): for each `mref`
  with a span and a located `FileId` (derive same as `push_span`, off the stored content
  address), push `WhereBytes { string: StringId::of(content[lo..hi]), file, lo, hi }`
  into a `ModuleRows.spans` batch → `insert_spine_where_bytes` (path set, so
  `retract_paths` prunes them).

### Forced: expose the span id in `ref`
`refresh_spine_rels` ([engine.rs:1066](../v5/src/engine.rs#L1066)) projects
`(string,file,lo,hi)` — no id. `edit(ref_id, …)` needs the coordinate named, so add the
id: `ref(id, string, file, lo, hi)`.

### A — Rust `--move OLD=NEW` driver + `edit` sink + drain
- New `edit(ref TEXT, new TEXT)` table: `ref` = `WhereBytesId`, `new` = `StringId`.
- Port `rewrite_use_path` + `reconvert_prefix` from archive
  [crates/watch/src/rs_path.rs:301,366]; `resolve_to_absolute` already exists at
  [modgraph.rs:533](../v5/src/modgraph.rs#L533). Add `file_to_mod_path`.
- `--move`: query `ref` for use-path spans under OLD → `rewrite_use_path(...)` → intern
  the O(1-5) new strings via one `insert_rows`/`insert_spine_strings` → insert `edit`
  rows.
- `drain_edits(apply)`: join `edit ⋈ _where_bytes ⋈ _strings` → `(path,lo,hi,new_text)`;
  group BY FILE; sort splices DESC by `lo` (apply from end backward so offsets stay
  valid); overlap guard (error on intersecting spans); under `--fix` splice + write,
  else unified diff. LSP rename = same drained set → `WorkspaceEdit`. Group key =
  span/`ref_id`, never resolved-path text.

### B — DEFER ENTIRELY
If a DSL rewrite rule is ever wanted: B-batched only (rule emits `edit(ref_id, raw_text)`
into staging; post-fixpoint drain interns text → ids in one batch, reusing A's
splice/write drain). Never ship a `rewrite_use` operator that interns per emitted row.

## Order
P0 → P1 (short-term, standalone correctness; commit) → reload → F1 → expose `ref.id` →
A. P0/P1 before F1 because M2 (incremental N+1) and M3 (retraction after `--fix`
rewrites a file) make them prerequisites once edit-spans exist.

## Key files
- `v5/src/spine.rs:86` WhereBytesId::of / new of_located (P0)
- `v5/src/engine.rs:606-616` incremental N+1 (P1); :1521 stored-id chokepoint (P0);
  :1131 module_rows_for_rev (F1); :1066 refresh_spine_rels (ref.id); :882 full-scan
  insert; :902 retract_paths
- `v5/src/modgraph.rs:34` ModuleRef; :307 expand_use; :533 resolve_to_absolute
- `sprefa-archive-20260428/crates/watch/src/rs_path.rs:301,366` rewrite_use_path/reconvert_prefix
- `v5/tests/spine_meta.rs:194,249,294` id recompute (P0 test updates)
