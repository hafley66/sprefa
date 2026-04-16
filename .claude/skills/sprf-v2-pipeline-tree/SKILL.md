---
name: sprf-v2-pipeline-tree
description: sprefa v2 dual-tree model — pipeline tree (SprfPath x captures) vs content tree (AnyDataNode). PathSeg encoding, span recursion rule, op_path-based hover dispatch in analysis.
---

# sprf v2 pipeline tree + content tree

## What this covers

Two address spaces share one cursor. Pipeline tree (`SprfPath`, runtime) coordinates op-hops, iter positions, fork arms. Content tree (parsed documents under `Slots`) coordinates source-byte structure. Captures on pipeline-tree leaves point into content-tree nodes via `byte_range`. LSP hover and completion consume the pipeline tree by walking `Pipeline` using an `op_path: Box<[usize]>` index path parallel to `SpanKind` entries.

## Current code (verified 2026-04-15)

- `SprfPath` at `v2/src/_0_types.rs:76-83`: `SprfPath(pub Arc<[PathSeg]>)`.
- `PathSeg` at `v2/src/_0_types.rs:85-93`. Variants: `Op { name, parse_site, step }`, `Named { name, key, parse_site }`, `ForkArm { index, parse_site }`, `SwitchArm { pat, parse_site }`, `LeafArm { key, parse_site }`, `Iter { index }`.
- `ParseSite` (compile-time coordinate) at `_0_types.rs:57-70`: `file`, `path: Arc<[ParseSeg]>`, `byte_range`. `ParseSeg` variants `Top`, `BraceChild`, `ParenChild`, `PatternLeaf`.
- Path tagging site: `Pipeline::run_with_step` at `v2/src/_5_op.rs:139-203`. `push_path` helper at `_5_op.rs:206-211`. Op hops push `PathSeg::Op { name, parse_site, step }`. Fork arms push `PathSeg::ForkArm { index, parse_site }`.
- `Pipeline` variants at `_5_op.rs:68-73`: `Op(LoweredOp)`, `Seq(Vec<Pipeline>)`, `Fork(Vec<ForkBranch>)`, `Switch { on, arms }`. `Switch` branch is `unimplemented!` at `_5_op.rs:201`.
- Pipeline-tree address for hover/LSP lives in `v2/src/analysis.rs`:
  - `SpanKind` at `analysis.rs:202-217` with `op_path: Box<[usize]>` on `OpName`, `Capture`, `CrossRef`, `MatchSite`.
  - `resolve_op_in_rule` at `analysis.rs:1124-1151` walks the Pipeline tree by index path, starting through the outer rule's Seq shell then descending into the RuleOp's `body_pipeline()`.
  - `find_binding_op_in_rule` / `find_binding_in_body` at `analysis.rs:1157-1220` walk strictly-earlier siblings inside a Fork arm to locate the most recent op whose `binds_captures()` contains a requested var.
  - `hover_at` at `analysis.rs:412-465` dispatches by `SpanKind`: `OpName` → `op.hover_self()`; `Capture` → upstream binder's `hover_capture`; `CrossRef` → target rule's first op `hover_capture`; `MatchSite` → `op.hover_match(site, cursors)` with non-match diagnostic append.
- Content tree: `AnyDataNode` at `v2/src/data/_0_types.rs` (checked via session source). Published by json under `JSON_TREE` slot. See `sprf-v2-cursor-slots`.

## Key invariants

- `SprfPath` is runtime state, one per cursor, stamped only by framework in `run_with_step`. Ops do not mutate `path`.
- `ParseSite` is compile-time, immutable once built by host parser. It is an `Arc`, cheaply shared across cursors that flow through the same op.
- Span arithmetic lives in the content domain (absolute byte offsets into `cursor.content`). Address/routing lives in the pipeline domain (`op_path`, `SprfPath` prefix).
- `op_path` encoding in `SpanKind` (from `analysis.rs:204-207`): a sequence of indices that walks the Pipeline tree; `[]` = root op, `[i, ...]` = descend into Seq child i then follow. Even-length paths are required at decode: pair `(fork_idx, op_idx)` per nesting level. See `analysis.rs:1145-1148`.
- Capture-hover delegation finds the binding op by walking strictly-earlier siblings in the enclosing Fork arm, matching `binds_captures()` — no op-name switching. See `find_binding_in_body` at `analysis.rs:~1180`.
- Fork today is buffer-and-replay (`block_on` collect, clone buffered batches into each arm). See `_5_op.rs:182-199`. Daemon-shaped broadcast/replay deferred.
- `Pipeline::Switch` is unimplemented at runtime; parsing it is present but running a Switch panics.

## How to use this skill

Load when: wiring a new LSP surface (hover, completion, diagnostic anchoring), designing `op_path`-keyed analysis, reasoning about fork/arm address space, adding new `PathSeg` variants, or mapping a source byte position to a pipeline-tree node. Answers: what shape `SprfPath` takes, where runtime addresses get stamped, how `op_path` maps into `Pipeline`.

## Landed vs deferred

Landed:

- `SprfPath` / `PathSeg` runtime trail with `Op`/`ForkArm` stamping.
- `SpanKind::{OpName, Capture, CrossRef, MatchSite}` with `op_path` resolution.
- Capture-hover delegation via `binds_captures()` upstream walk (commit `180daf3`).
- Field-segment hover for `&.fs`/`&.repo`/`&.rev` via `CursorRefOp::hover_match` (commit `180daf3`).
- Fork-arm path tagging (`PathSeg::ForkArm { index, parse_site }`).
- `OpEvidence` append on each op hop when `config.runtime.collect_witnesses`.

Deferred (session `20260415.5` described these; not in code):

- Runtime `SprfPathPattern` + `$ROOT` / `$SELF` / `$PARENT` anchors + wildcard `[*]`. Today's `PathExpr` (see `sprf-v2-cursor-ref`) has `SelfCap` + `Field` only.
- `span($PATH)`, `bytes($PATH.$NAME)`, `value($PATH.$NAME)` functions over addressed nodes.
- `$ROOT.fs[0].json[1].$CODE` global cross-hop addressing.
- `@label` named arms + `Pipeline::Switch` runtime + `PathSeg::Named` use at runtime (the variant exists; nothing builds one yet).
- Reactive re-eval from PathPattern-edges.
- Walker `MatchResult.match_span` producing subtree-root span. Today `byte_range` on walker-emitted rows is a capture-union proxy (`row_byte_span` at `ops/_5_json.rs:801`). Described as "match_span debt" in session.
- LSP completion on `&.` popup over the pipeline address space. `CursorRefFactory` returns a single static `&.$CAP` completion item today (`ops/_6_cursor_ref.rs:39-45`).

## Drift notes

- Session `20260415.5` used `SprfPathPattern` as a design name for a compile-time path query type. The shipped compile-time type is `PathExpr` at `v2/src/path_expr.rs:26-39` — scope limited to `SelfCap(Arc<str>) | Field(FieldKind)`. Treat `SprfPathPattern` as the forward-direction name of the richer successor.
- Session proposed `OpArg::{Literal, CursorRef}` for paren-argument shape. Code did NOT take that form; `&.` became its own pipe-step op (`cursor_ref`) with its own `parse_site`. See `sprf-v2-cursor-ref` for the replacement.
- Session proposed `Op::redirectable_inputs()` opt-in. Rejected in favor of op-slot framing (see session `20260415.7`). No such method on `Op`.
- `PathSeg::Named` exists in `_0_types.rs:88` but nothing builds one at runtime; `@label` arms are design-only.

## Related

- `sprf-v2-op-trait-family` — `Pipeline::run_with_step` is the path-tagging site.
- `sprf-v2-cursor-slots` — the `byte_range`/slots half of the dual-tree model.
- `sprf-v2-cursor-ref` — `&.$X` consumer of the (today narrow) runtime path vocabulary.
- Session: `chat_log/20260415.5.pipeline-tree-and-cursorref.md`.
- Memory: `project_v2_path_tagging.md`, `project_v2_runner_owns_analysis.md`.

## Provenance

Session: `chat_log/20260415.5.pipeline-tree-and-cursorref.md`
HEAD at skill write: 180daf3 feat(v2): cursor_ref hover delegation + field-segment render
Verified by: Claude Opus 4.6 (1M) on 2026-04-15
