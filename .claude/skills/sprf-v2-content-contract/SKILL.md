---
name: sprf-v2-content-contract
description: sprefa v2 content source-of-truth contract. Every byte-reading op MUST parse cursor.content[byte_range or all] first; reader.bytes(c.fs) is fallback only. Required by cursor_ref to function end-to-end.
---

# sprf v2 content source-of-truth contract

## What this covers

Every op that reads bytes treats `cursor.content` (sliced by `cursor.byte_range` if set) as authoritative. The `Reader` is only the populate path used when `content` is `None`. Without this, `&.$X` rebase is silently broken: a downstream op re-fetches the OUTER file via `reader.bytes(c.fs)`, ignoring the rebase. The first op to honor the contract is `JsonOp`. Every future byte-reading op (ast-grep, md, line) owes the same dispatch order.

## Current code (verified 2026-04-15)

- Reference implementation: `JsonOp::pipe` candidate-builder block in `v2/src/ops/_5_json.rs`.
  - Content-priority branch around `ops/_5_json.rs:380-420`. Shape: `if let Some(content) = c.content.as_ref() { let bytes = match &c.byte_range { Some(r) => content.slice(r.clone()), None => content.as_ref().clone() }; let fp = c.fs.clone().unwrap_or_else(|| FilePath(Arc::from(Path::new("<rebased>")))); vec![(fp, bytes)] } else { /* reader fallback */ }`.
  - Extension fallback at `ops/_5_json.rs:437-442`: when `fp` has no recognized json/yaml/toml extension (e.g. synthesized `<rebased>` path), default to `"json"` so `parse_by_ext` still picks a parser.
  - Fresh-parse path also stamps `c2.content = Some(content_arc.clone())` on every emitted cursor (`ops/_5_json.rs:~482`) so the next op down can rely on `c.content` being set.
- Walker `CaptureAny` step at `v2/src/walk/_3_walker.rs:263-288`. Matches any node kind (scalar, object, array) and captures the byte range. Replaces the previous `Leaf` semantics for bare `$VAR` in value position; without it, `$OBJ` with an object value silently dropped to zero captures.
- Walker `is_row_field` at `v2/src/walk/_3_walker.rs:349-354` accepts `Leaf | LeafPattern | CaptureAny` so multi-key captures `{a:$A, b:$B, c:$C}` merge into one row instead of emitting three.
- Brace parser `v2/src/walk/_4_brace_parse.rs:241-247`: bare `$VAR` lowers to `SelectStep::CaptureAny`; cross-refs `${rule.$VAR}` keep `SelectStep::Leaf` for constraint-matching against pre-seeded scalar values.
- `CompiledStep::CaptureAny` variant at `v2/src/walk/_1_compiled.rs:48-51`.
- `SelectStep::CaptureAny` at `v2/src/walk/_2_compile.rs:48`, lowered to `CompiledStep::CaptureAny` at `_2_compile.rs:87-89`.
- Test pinning the contract: `v2/tests/cursor_ref.rs:78-139` (`cursor_ref_span_backed_narrows_byte_range`). Second half runs `json({pkg:$PKG}) > &.$PKG > json({version:$V})` and asserts `V="1.2.3"` (the inner version field exists at the top level of the sub-object, not at the top level of the outer file). If the contract regresses the assert fails.
- The chained inner json must also re-publish `JSON_TREE` on its output (`tests/cursor_ref.rs:137-138`).

## Key invariants

- Dispatch order for every byte-reading op:
  1. PATH A — slot-reuse fast path: if a typed parse-tree slot is set and applicable, walk the pre-parsed tree.
  2. PATH B — content-priority: if `c.content` is `Some`, parse `content[byte_range or 0..]`.
  3. PATH C — reader fallback: if `c.content` is `None`, call `reader.bytes(c.repo, c.rev, c.fs)`.
- Wildcard mode (no `c.fs`) uses the reader path to enumerate files via `reader.files(...)`.
- Synthesized `fs` paths (rebased cursors with no original `fs`) get a parser-extension default per op; `json` defaults to `"json"`.
- Walker `Leaf` step requires a scalar (`as_scalar_text() → Some`). Any pattern position that may capture object/array values must use `CaptureAny`, not `Leaf`.
- Cross-refs `${rule.$VAR}` keep `Leaf` semantics — they need scalar-text constraint matching against pre-seeded values; `CaptureAny` would skip the constraint check for non-scalars (`_3_walker.rs:271-274`).
- After json's fresh-parse path, every emitted cursor carries `c.content = Some(content_arc.clone())`. Downstream ops can rely on this.

## How to use this skill

Load when: porting any byte-reading op from v1 (ast-grep, md, line, custom scanners), debugging "downstream op silently parses the wrong bytes", touching the json candidate-builder block, or designing the next op's dispatch order. Answers: what dispatch order satisfies the contract, what the reference implementation looks like, what test catches regressions.

## Landed vs deferred

Landed (commit `180daf3` consolidated session-.7 + session-.8 work):

- json honors PATH A/B/C dispatch order.
- json fresh-parse path stamps `c2.content` on every emitted cursor.
- Extension fallback for synthesized `<rebased>` paths.
- Walker `CaptureAny` step + `is_row_field` extension.
- Bare `$VAR` lowers to `CaptureAny`; cross-refs keep `Leaf`.
- Chained-json end-to-end test pinning the contract for the json case.

Deferred (session `20260415.8` listed; not in code):

- ast-grep op port — first non-json content-tree consumer; will need PATH A/B/C dispatch.
- md op.
- line op (canonical chain `json({code:$C}) > &.$C > line(re:/TODO/)` from session `.7`).
- Wildcard-mode JSON_TREE publication (deferred at commit `4b94025`).
- Walker `MatchResult.match_span` to replace capture-union proxy.
- Per-op chained tests when porting (the session-.8 author noted: "this test will catch the json case but not the new op — add an analogous chained test when porting each op").

## Drift notes

- Session `20260415.8` listed two open hover items as `[ ]` unchecked. Both shipped in commit `180daf3`:
  - Capture-token hover delegation: `analysis.rs:433-443` + `find_binding_op_in_rule` at `analysis.rs:1157-1220`.
  - Field-segment hover: `CursorRefOp::hover_match` at `ops/_6_cursor_ref.rs:92-104`.
  Treat session-.8's task list as closed for hover; the deferred list above is the residue.
- Session `20260415.8` "Open Questions" item about `read > json` (no rebase between) producing one fewer reader call after the contract fix: confirmed behavior shipped, not pinned by a dedicated test.
- Session described the contract using the verbatim quote from session `20260415.7` lines 71-73. The shipped code in `ops/_5_json.rs` makes it real for the json case.

## Related

- `sprf-v2-cursor-ref` — the rebase mechanic that needs this contract to function end-to-end.
- `sprf-v2-cursor-slots` — `cursor.content` and `cursor.byte_range` field semantics.
- Session: `chat_log/20260415.8.cursorref-content-contract.md` (this contract's landing memo).
- Session: `chat_log/20260415.7.cursorref-as-op-slot.md` lines 71-73 (the contract promise).
- Memory: `project_content_byte_range_contract.md`.
- Commit: `180daf3 feat(v2): cursor_ref hover delegation + field-segment render` (closed session-.8 hover gap).

## Provenance

Session: `chat_log/20260415.8.cursorref-content-contract.md`
HEAD at skill write: 180daf3 feat(v2): cursor_ref hover delegation + field-segment render
Verified by: Claude Opus 4.6 (1M) on 2026-04-15
