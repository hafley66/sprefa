---
name: sprf-v2-op-trait-family
description: sprefa v2 Op and Operator trait surfaces, cursor/op ownership rules, static-first dispatch posture, and reserved slots for future effects/schema/projections work.
---

# sprf v2 Op trait family

## What this covers

Trait family posture for v2. Ops own their code, config, diagnostics, reads, hover surface, captures, scan pointers, and pattern-driven completion. Core is a thin spine: registry, parser, lowering, DAG, path tagging, cursor clone. Static dispatch today via `Arc<dyn Op>`; dyn-Any escape hatch for cursor payload (Slots). No central enum of op identities.

## Current code (verified 2026-04-15)

- `Op` trait at `v2/src/_5_op.rs:316-379`. Methods that exist today:
  - `pipe(input: BoxStream<'static, Arc<[Cursor]>>, ctx: OpCtx) -> BoxStream<'static, Arc<[Cursor]>>` — `_5_op.rs:317-321`. Stream shape is batched `Arc<[Cursor]>`, not single cursor.
  - `name`, `step`, `parse_site` — `_5_op.rs:323-325`.
  - `tokens`, `hover_at` — `_5_op.rs:326-327`.
  - `witness(&Cursor) -> Option<Arc<str>>` — `_5_op.rs:332`. Default `None`.
  - `witness_insert` — `_5_op.rs:337-339`.
  - `capture_name` — `_5_op.rs:343`.
  - `hover_self`, `hover_capture`, `hover_match` — `_5_op.rs:350-359`.
  - `body_pipeline`, `with_body` — `_5_op.rs:364-371`.
  - `binds_captures() -> Vec<Arc<str>>` — `_5_op.rs:376-378`. Default delegates to `capture_name()`; walker ops override.
- `Operator` trait at `v2/src/_5_op.rs:501-550+`:
  - `name`, `aliases` — `_5_op.rs:502-503`.
  - `bracket_grammar`, `paren_grammar`, `brace_mode` — `_5_op.rs:505-507`.
  - `pre_register`, `parse` — `_5_op.rs:509-513`.
  - `scan_pointers() -> &'static [ScanPointer]` — `_5_op.rs:517`. `ScanPointer { sigil, read: fn(&Cursor) -> Option<Arc<str>> }` at `_5_op.rs:496-499`.
  - `completion_item`, `valid_at_head`, `head_suggestions`, `wildcard_instance` — `_5_op.rs:520-550`.
- `LoweredOp { op: Arc<dyn Op>, xrefs: Arc<[CrossRefOccurrence]> }` — `_5_op.rs:91-106`. Wraps an op with its compile-time cross-ref bindings so `expand_xrefs` splices source-row seeding before `pipe`.
- `OpCtx` at `_5_op.rs:273-296`: `reader`, `writer`, `config`, `diags` (DiagSink), `events` (EventSink), `result_store: Arc<ResultStore>`, `xref_seen`.
- `Pipeline` enum — `_5_op.rs:68-73`: `Op(LoweredOp)`, `Seq(Vec<Pipeline>)`, `Fork(Vec<ForkBranch>)`, `Switch { on, arms }`. `Switch` is `unimplemented!` at `_5_op.rs:201`.
- `Pipeline::run_with_step` — `_5_op.rs:139-203`. Framework-owned: appends `PathSeg::Op { name, parse_site, step }`, records `OpEvidence` via `witness()` when `config.runtime.collect_witnesses`, tags `PathSeg::ForkArm { index, parse_site }` on arm outputs. Fork today uses `block_on` + full buffer replay; `_5_op.rs:182-199`.
- Op registration via `v2/src/ops/mod.rs:default_registry()` (`_20-30`). Seven factories: `RuleFactory`, `RepoFactory`, `RevFactory`, `FsFactory`, `ReadFactory`, `JsonFactory`, `CursorRefFactory`.

## Key invariants

- Ops never call `tokio::spawn`, `buffer_unordered`, or own a thread pool. Runner policy controls concurrency; ops return streams.
- Ops never read config field matches on op-name (`match "repo" | "rev"` anywhere in core is a violation). `scan_pointers()` on `Operator` is the registration slot for what were `$$repo`/`$$rev` hardcodes.
- Default impls on every method that isn't load-bearing. Small ops stay a handful of lines. Larger ops opt in by override.
- Cursor clone is mandatory per hop (framework appends `PathSeg` via `push_path` at `_5_op.rs:206-211`). Op bodies must treat `&Cursor` as read-only and rebuild owned copies for emission.
- `binds_captures()` is the authoritative scope predicate for capture-token hover delegation and LSP scope. Ops that populate captures through a walker must override (see `JsonOp::binds_captures` at `ops/_5_json.rs:215-220`).
- `LoweredOp` is the pipeline storage unit, not `Arc<dyn Op>` alone. `Arc::new(MyOp).into()` funnels through `From<Arc<T>>` at `_5_op.rs:111-113`.

## How to use this skill

Load when: adding a new op, touching any trait method signature on `Op`/`Operator`, wiring a new hover/completion dispatch path, designing runner parallelism, or reasoning about "where does X live — op or core?". Answers: what defaults exist, what names have been taken, which trait slot a new feature belongs in.

## Landed vs deferred

Landed:

- `Op::pipe` on batched `Arc<[Cursor]>` stream.
- `Op::binds_captures()` with walker override point.
- `Op::hover_self`/`hover_capture`/`hover_match` surface; analysis dispatcher routes via `SpanKind` (`v2/src/analysis.rs:202-217`, `412-465`).
- `Operator::scan_pointers()` op-owned `$$sigil` registration.
- `LoweredOp` + `expand_xrefs` shipping cross-ref seeding outside op code.
- `Operator::wildcard_instance` for partial-eval LSP completion.

Deferred (session `20260414.5` names these; still not in code):

- `Op::reads(&Cursor) -> Vec<Arc<dyn ReadIntent>>` — declarative read intents. Reads still live inline in `pipe` bodies.
- `Op::effects(&Cursor) -> Vec<Arc<dyn Effect>>` — write effect bus. No Layer 5 yet.
- `Operator::schema() -> OpSchema` with `allowed_parents`/`allowed_children`/`arity` for a DOM-validator. `valid_at_head` is the closest shipped primitive.
- `Operator::projections() -> &'static [Projection]` — generalized provenance projection. `scan_pointers` shipped a narrower version.
- `Operator::init_config(&mut ConfigSlot, &OpInvocation)` — op-owned config reducer.
- Codegen of typed Cursor fields via `marker()`/`render()` harvest. `Slots` dyn-Any map (see `sprf-v2-cursor-slots`) is the shipped substitute.
- Static enum `PipelineOp { Rule, Repo, ... }` as replacement for `Arc<dyn Op>`. Pipeline still stores trait objects.
- `Pipeline::Switch` runtime.

## Drift notes

- Session `20260414.5` listed `v2/src/_5_op.rs::run_with_step` as the path-tagging site. Confirmed — function exists at `_5_op.rs:139`.
- Session `20260414.5` proposed `OpSchema`, `projections`, `reads`, `effects`, `init_config` as trait slots. None have landed. The session is a design memo; treat as forward direction.
- `feedback_op_owns_everything.md` memory: aligned with shipped code — ops own diagnostics, patterns, fix-hints, captures, scan pointers, completion, hover. Confirmed.
- Session mentions `v2/src/_7_runner.rs`, `v2/src/_10_registry.rs`, `v2/src/_11_dag.rs` as landing sites for deferred effect/reads work. Those files exist today; the hooks do not.

## Related

- `sprf-v2-cursor-slots` — the dyn-Any escape hatch that shipped in place of codegen typed slots.
- `sprf-v2-pipeline-tree` — the `PathSeg`/SprfPath coordinate system that path tagging produces.
- `sprf-v2-content-contract` — the source-of-truth rule every byte-reading op must follow.
- Session: `chat_log/20260414.5.v2-op-trait-family-design.md`.
- Memory: `project_op_trait_family.md`, `feedback_op_owns_everything.md`, `feedback_op_owned_annotations.md`, `project_v2_runner_parallelism.md`, `project_v2_runner_owns_analysis.md`.

## Provenance

Session: `chat_log/20260414.5.v2-op-trait-family-design.md`
HEAD at skill write: 180daf3 feat(v2): cursor_ref hover delegation + field-segment render
Verified by: Claude Opus 4.6 (1M) on 2026-04-15
