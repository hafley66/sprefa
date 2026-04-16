---
name: sprf-v2-cursor-ref
description: sprefa v2 cursor_ref op (&.$X / &.fs / &.repo / &.rev). Grammar desugar, PathExpr type, CaptureKind-driven rebase mechanic, hover delegation. The bash $() of sprf.
---

# sprf v2 cursor_ref op (`&.$X`)

## What this covers

`&.<path>` is a pipe-step that rebases the cursor onto a captured sub-content or a cursor field. It is its own `Op` impl, not an inline argument to other ops. Grammar parses `&.<path>` and desugars it to `OpInvocation { name: "cursor_ref", paren_src: ".<path>" }`. Runtime calls `cursor.rebase(&capture)` which dispatches on `CaptureKind`. Slots clear on rebase. Hover delegates capture-token semantics to the upstream binder; field segments render per-cursor grouped output.

## Current code (verified 2026-04-15)

- `CursorRefFactory` and `CursorRefOp` at `v2/src/ops/_6_cursor_ref.rs:29-156`.
  - Factory `name()` returns `"cursor_ref"` (`_6_cursor_ref.rs:32`).
  - `valid_at_head` returns `false` (`_6_cursor_ref.rs:37`) — op is unreachable by name; only via `&.` desugar.
  - `parse` builds a `PathExpr` from `inv.paren_src.src` (`_6_cursor_ref.rs:47-67`).
  - `pipe` matches on `PathExpr::SelfCap` and `PathExpr::Field` per cursor and emits `cursor.rebase(&cap)`. Diags drop the cursor on miss. (`_6_cursor_ref.rs:106-155`).
  - `hover_match` renders header `&{path.render()}` and per-cursor grouped output via `hover_render_grouped` (`_6_cursor_ref.rs:92-104`).
  - Diagnostics: `cursor_ref/missing-path`, `cursor_ref/bad-path`, `cursor_ref/unknown-capture`, `cursor_ref/field-unset` (`_6_cursor_ref.rs:162-202`).
- `PathExpr` at `v2/src/path_expr.rs:26-39`. Two variants today: `SelfCap(Arc<str>)`, `Field(FieldKind { Repo | Rev | Fs })`.
  - `parse(src)` accepts `.$NAME`, `.repo`, `.rev`, `.fs` (`path_expr.rs:48-72`).
  - `render()` round-trips (`path_expr.rs:75-85`).
  - `resolve(&Cursor) -> Option<Cow<'_, str>>` (`path_expr.rs:91-110`).
- Grammar desugar in `v2/src/_8_parse.rs:292-345`. Loop sees `&` byte at pipe-step position, collects path body until `>`, `;`, `{`, `}`, or whitespace, builds `OpInvocation { name: "cursor_ref", paren_src: Some(ParenSlot { src: ".<path>", ... }), parse_site, ... }`. The `&` token never hits the ident-start path.
- `Cursor::rebase` at `v2/src/_0_types.rs:288-303`. Per `CaptureKind`:
  - `SpanBacked { span }` → `out.byte_range = Some(span); out.slots = Default;` content unchanged.
  - `Synthesized` → `out.content = Arc::new(Bytes::copy_from_slice(cap.value.as_bytes())); out.byte_range = None; out.slots = Default;`.
- `CaptureKind` at `v2/src/_0_types.rs:174-183`. Stamped at json emit time by `classify_capture` (`v2/src/ops/_5_json.rs:784`): first non-whitespace byte `{` or `[` → SpanBacked, else Synthesized. JSON strings (already unescaped in `wc.text`) are Synthesized.
- Field-path `&.fs` materializes `cursor.fs` as new content via the Synthesized path. `PathExpr::Field` resolves to `Cow<str>`, `pipe` wraps in `Capture::new` (Synthesized default), then rebases (`_6_cursor_ref.rs:135-149`).
- Hover delegation lives in `analysis.rs::hover_at` `Capture` arm at `analysis.rs:433-443`. When the site_op does not bind the capture, walks earlier siblings via `find_binding_op_in_rule` and calls that op's `hover_capture`. Field segments hit `MatchSite` and call `CursorRefOp::hover_match`.
- Tests:
  - `v2/tests/cursor_ref.rs` (251 lines). Four tests: `cursor_ref_span_backed_narrows_byte_range` (covers rebase shape AND chained downstream json consumption asserting `V="1.2.3"`), `cursor_ref_synthesized_materializes_unescaped_content`, `cursor_ref_field_fs_materializes_path`, `json_emits_correct_capture_kinds`.
  - `v2/tests/hover_render.rs:455-491`: `hover_delegates_capture_to_binding_op` and `hover_cursor_ref_field_fs`. Both pass via `DocSession::hover_at`.

## Key invariants

- `&.` at pipe-step position desugars to a `cursor_ref` invocation. No other consumer of the `&` byte at that position. `&` followed by non-`.` content yields whatever path-body collection produces, then `PathExpr::parse` errors.
- Slot rule: slots survive iff content unchanged. SpanBacked rebase clears slots anyway (current `rebase` impl) because the existing JSON_TREE root spans the whole file; downstream re-parses through the content-priority dispatch path.
- Captures are SpanBacked iff their raw bytes are valid source bytes for the same parser (JSON object/array). Strings, numbers, booleans, null are Synthesized.
- `cursor_ref` does not bind a new capture — `binds_captures()` returns empty (`_6_cursor_ref.rs:90`). Downstream capture scope is unchanged by the rebase.
- `cursor_ref` is unreachable by typing `cursor_ref(...)`; `valid_at_head` is `false`. Sole entry is the grammar desugar.
- Hovering `$NAME` inside `&.$NAME` delegates to the op that bound `NAME` upstream. Hovering `.fs`/`.repo`/`.rev` calls `CursorRefOp::hover_match` which enumerates per-cursor resolved values via `hover_render_grouped`.

## How to use this skill

Load when: implementing or modifying `&.` semantics, debugging "downstream op silently re-parses outer file" (the content-contract path; see `sprf-v2-content-contract`), adding a new `PathExpr` variant, or reasoning about how a capture flows from json into a downstream parser. Answers: what `&.` means, how `CaptureKind` chooses rebase mode, where hover dispatches.

## Landed vs deferred

Landed:

- Grammar `&.$NAME`, `&.repo`, `&.rev`, `&.fs` desugar.
- `PathExpr::SelfCap` + `Field(FieldKind)` parse/render/resolve.
- `CursorRefFactory` registered in `default_registry()`.
- `CursorRefOp::pipe` SpanBacked + Synthesized rebase.
- `Cursor::rebase` two-path semantics.
- `classify_capture` byte probe stamping CaptureKind at json emit time.
- Capture-token hover delegation via `find_binding_op_in_rule` (commit `180daf3`).
- Field-segment hover via `CursorRefOp::hover_match` (commit `180daf3`).
- Four-test integration suite + two end-to-end hover tests.

Deferred (sessions `20260415.7` and `20260415.8` listed; not in code):

- Walker `MatchResult.match_span` to replace capture-union span proxy.
- ast-grep, md, line ops as second/third/fourth content-contract consumers.
- Wildcard-mode JSON_TREE publication (deferred at commit `4b94025`).
- `PathExpr::Chain(Vec<PathSeg>)` global paths like `.$ROOT.fs.0.json.1.$NAME`.
- Array index path segments (`.0`).
- Named cursor_ref variants (`&ref`, `&narrow`, `&materialize`). Sole sigil is `&`.
- LSP completion for `&.` popup over upstream captures + cursor fields. Static label only today.

## Drift notes

- Session `20260415.5` first proposed `&.$X` as inline op argument with `Op::redirectable_inputs()` opt-in. Session `20260415.7` superseded that with op-slot framing. Code matches `.7`: dedicated `cursor_ref` op, no `redirectable_inputs` method on `Op`.
- Session `20260415.5` proposed `OpArg::{Literal, CursorRef}` enum. Not in code.
- Session `20260415.5` named the path-pattern type `SprfPathPattern`. Code shipped `PathExpr` (`v2/src/path_expr.rs`) with a much narrower variant set.
- Session `20260415.7` listed the grammar landing site as `v2/src/_15_pipeline_rewrite.rs` ("likely"). Actual landing site is `v2/src/_8_parse.rs:292-345`. `_15_pipeline_rewrite.rs` does exist but the desugar lives in the host-parse pipe-step loop.
- `project_cursor_ref_plan.md` line 41 lists future homes for PathExpr conditionals. None of (a) filter op, (b) pattern bleed, (c) bracket-slot conditions are landed.

## Related

- `sprf-v2-content-contract` — without it the rebase is useless because downstream ops re-fetch via `reader.bytes(c.fs)`.
- `sprf-v2-cursor-slots` — `Cursor::rebase` consumer; CaptureKind shape.
- `sprf-v2-pipeline-tree` — hover dispatch via `op_path` + `find_binding_op_in_rule`.
- Sessions: `chat_log/20260415.5.pipeline-tree-and-cursorref.md` (superseded design), `chat_log/20260415.7.cursorref-as-op-slot.md` (landing target), `chat_log/20260415.8.cursorref-content-contract.md` (contract enforcement).
- Memory: `project_cursor_ref_plan.md`, `project_content_byte_range_contract.md`, `project_cursor_ref_hover_gap.md`.
- Commit: `180daf3 feat(v2): cursor_ref hover delegation + field-segment render`.

## Provenance

Session: `chat_log/20260415.7.cursorref-as-op-slot.md`
HEAD at skill write: 180daf3 feat(v2): cursor_ref hover delegation + field-segment render
Verified by: Claude Opus 4.6 (1M) on 2026-04-15
