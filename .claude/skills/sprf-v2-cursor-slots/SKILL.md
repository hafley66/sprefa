---
name: sprf-v2-cursor-slots
description: sprefa v2 Cursor.slots + SlotKey<T> typed payload channel, byte_range narrowing protocol, and active_bytes() helper. How parsed trees flow between ops by Arc without deep copy.
---

# sprf v2 Cursor slots + byte_range

## What this covers

`Cursor` carries a dyn-Any-keyed typed payload store (`Slots`) plus a runtime `byte_range: Option<Range<usize>>` window into `content`. Ops publish parse trees under `pub const X: SlotKey<Wrapper> = SlotKey::new();`, downstream ops read them by Arc without re-parsing. `byte_range` narrows the active scan window of an op. One rule replaces a would-be codegen typed-struct approach until `marker()`/`render()` ship.

## Current code (verified 2026-04-15)

- `SlotKey<T>` and `Slots` in `v2/src/_0_types.rs:99-158`.
  - `SlotKey<T>` zero-sized phantom at `_0_types.rs:103-118`. Uses `PhantomData<fn() -> T>` so the key does not inherit `T`'s auto-traits.
  - `Slots` wraps `HashMap<TypeId, Arc<dyn Any + Send + Sync>>` at `_0_types.rs:123-126`. Derives `Default`, `Clone`, `Debug`.
  - API: `set`, `set_arc`, `get`, `contains`, `remove`, `is_empty`, `len` at `_0_types.rs:128-158`. All bound `T: 'static + Send + Sync`.
- `Cursor` fields at `v2/src/_0_types.rs:219-236`:
  - `byte_range: Option<Range<usize>>` — `_0_types.rs:233`.
  - `slots: Slots` — `_0_types.rs:235`.
- `Cursor::active_bytes(&self) -> &[u8]` at `_0_types.rs:260-266`. Returns `content[byte_range]` when both set, full `content` when range is `None`, empty slice when content is absent.
- `Cursor::get_slot`/`set_slot` shortcuts at `_0_types.rs:269-276`.
- `Cursor::rebase(&self, cap: &Capture) -> Cursor` at `_0_types.rs:288-303`. SpanBacked narrows `byte_range`, Synthesized materializes new `content`, both clear `slots`.
- Reference publisher: `JsonOp` at `v2/src/ops/_5_json.rs`.
  - `JsonTree` newtype and `pub const JSON_TREE: SlotKey<JsonTree>` at `ops/_5_json.rs:35-46`.
  - Publication sites: fresh-parse path around `ops/_5_json.rs:477-500`; slot-reuse path writes content + slot as part of per-row `Cursor` build.
- Every `Cursor { ... }` construction sets `slots: Slots::default(), byte_range: None`. `Cursor::default()` at `_0_types.rs:238-254` is the primary spread source; constructors that need only scalar fields pick it up.

## Key invariants

- `byte_range` is always an index into `content`. A cursor with `Some(content), None` is whole-file; `Some(content), Some(r)` is `content[r]`; `None, Some(r)` is a bug.
- Slot reads return `Option<Arc<T>>`, never `&T`. Caller gets cheap clone of the stored Arc; avoids tying a borrow to `Cursor::slots` across `.map` closures.
- Last-write-wins on slot insertion. Two ops publishing the same `SlotKey<Wrapper>` do not collide — the later write overwrites, which is the intended semantic for nested walks that narrow.
- Newtype-per-slot discipline is by convention, not enforced. Two ops declaring `SlotKey<AnyDataNode>` would collide silently on the same `TypeId`. Each publisher wraps: `struct JsonTree(Arc<AnyDataNode>)`.
- Slots must hold `T: 'static + Send + Sync`. Self-referential trees (oxc arena + AST, tree-sitter with source-borrow) need a self-ref wrapper (ouroboros / self_cell) before they can go into a slot.
- `Cursor::rebase` clears slots unconditionally. The parse tree was rooted at the whole-file content; sub-range narrowing invalidates it. Downstream re-parses against the rebased bytes.
- `cursor.content` sliced by `cursor.byte_range` is the source of truth for byte-reading ops. See `sprf-v2-content-contract`.

## How to use this skill

Load when: adding a parser-style op that wants to share its parsed tree downstream, designing a new capture shape, touching `Cursor` fields, or wondering where a parse tree lives on a cursor. Answers: what `SlotKey<T>` looks like in practice, how to publish without colliding, how `byte_range` composes with slot publication.

## Landed vs deferred

Landed:

- `Slots` dyn-Any map + `SlotKey<T>` typed handle.
- `byte_range` + `active_bytes()` helper.
- `JSON_TREE` slot published by `JsonOp` on emit.
- `Cursor::rebase` with two-path semantics per `CaptureKind`.
- `CaptureKind { SpanBacked { span }, Synthesized }` on `Capture` at `_0_types.rs:174-183`.

Deferred (session `20260415.4` listed these; still not in code):

- `line` op (first byte-range-only consumer).
- `ast-grep` / `md` ops as tree publishers with their own `SlotKey`s.
- Nested-json slot reuse (inner `json` reads upstream `JSON_TREE` restricted to current `byte_range` instead of re-parsing). Today's inner path hits the content-priority branch and re-parses.
- Codegen escape-hatch monomorphization of slot layout. `Slots` is the shipped substitute.
- Walker `MatchResult.match_span` surfacing the true anchor-node span. Today `row_byte_span` (`ops/_5_json.rs:801`) is a capture-union proxy.
- Interior-mutability cache slots (`SlotKey<Arc<RwLock<Cache>>>`). API already supports it; no consumer yet.

## Drift notes

- Session `20260415.4` listed 9 `Cursor { ... }` construction sites requiring `slots: Slots::default(), byte_range: None` updates. All updates landed; `Cursor::default()` absorbs most of them today.
- Session proposed `origin_byte_range` as a sibling of `byte_range` for hover recovery after Synthesized rebase. Not in code. `CaptureKind::SpanBacked` carries the span on the `Capture` instead; Synthesized rebase does lose original-file anchoring.
- Session proposed optional `LANG: SlotKey<Lang>` cross-op hint for md/ast-grep. Not in code.
- `CaptureKind` is session-.4 terminology ("kind" / content-type). Code landed the exact enum as `SpanBacked { span } | Synthesized` at `_0_types.rs:174-183`.

## Related

- `sprf-v2-content-contract` — the PATH A/B/C dispatch every byte-reading op owes.
- `sprf-v2-cursor-ref` — `cursor.rebase` consumer; CaptureKind determines rebase path.
- `sprf-v2-pipeline-tree` — how `byte_range` composes with SprfPath address space.
- Session: `chat_log/20260415.4.slots-slotkey-design.md`.
- Memory: `project_op_owned_cursor_slots.md`, `project_content_byte_range_contract.md`.

## Provenance

Session: `chat_log/20260415.4.slots-slotkey-design.md`
HEAD at skill write: 180daf3 feat(v2): cursor_ref hover delegation + field-segment render
Verified by: Claude Opus 4.6 (1M) on 2026-04-15
