# Plan: FS streaming — bound queue-peak to batch_cap

## Status — what's already in place

`FsComponent::dispatch` (`v4/src/v2_ops.rs:284-650`) ALREADY chunks emission:

- `buf: Vec<Node> capacity = self.batch` (line 435)
- `flush()` closure that calls `splice_into_at(parent, Node::Many(buf), ..., next_idx)`, threading the running batch_idx (lines 437-456)
- `if buf.len() >= self.batch { flush() }` (line 516)

So FS does NOT emit one giant Many. It emits N chunks of `self.batch` rows, each chunk a separate splice into the queue.

## What's NOT bounded

Dispatch itself is atomic. One call to `FsComponent::dispatch` walks the entire corpus to completion, pushing every chunk to the queue, then returns. Queue peak = `O(corpus)` rows, not `O(self.batch)`.

The expand loop only advances to the next depth AFTER `dispatch` returns. So ast cannot drain queue rows while FS is still emitting them, even though both run inside the same expand tick.

## Goal

Peak queue residence bounded by `batch_cap` regardless of corpus size. For Linux (63k files) today the cap is 65536 → one chunk, no issue. For a 10M-file corpus at the same cap, queue holds 10M rows ≈ 2GB. Bound the peak.

## Type signatures — what to change

### 1. FsComponent state

```rust
pub struct FsComponent {
    // ... existing fields ...
    /// Per-input-row walk continuation, keyed by parent queue id.
    /// Walker state persists across dispatch calls so a yield-resume
    /// can pick up at the next entry instead of starting over.
    walkers: Mutex<HashMap<QueueId, FsWalkState>>,
}

struct FsWalkState {
    walker: Box<dyn Iterator<Item = ignore::DirEntry> + Send>,
    next_idx: u32, // running batch_idx for splice_into_at
    parent: QueueRow<Cursor>, // cached so resume re-uses the same parent ref
}
```

### 2. Dispatch contract

```rust
fn dispatch(&self, ctx: &RenderCtx, rows: &[QueueRow<Cursor>], queue: &dyn QueueBackend<Cursor>) {
    for parent in rows {
        // Resume from continuation OR start fresh
        let mut state = self.walkers.lock().unwrap()
            .remove(&parent.id)
            .unwrap_or_else(|| FsWalkState::new(parent, walk_root));

        // Emit ONE chunk of size <= batch_cap
        let chunk = take_one_chunk(&mut state.walker, ctx.opts.batch_cap);

        if chunk.is_empty() {
            // Walker exhausted — emit Done semantics implicitly (no
            // resume row enqueued).
            continue;
        }

        // Splice the chunk into the queue at depth + 1
        let many = Node::Many(chunk);
        let n = splice_into_at(parent, many, ctx.depth + 1, ctx.expand_tick, queue, state.next_idx);
        state.next_idx += n as u32;

        // Re-enqueue a YIELD at the SAME depth so expand calls us
        // back after ast has drained this chunk. The yield-cursor
        // is the same parent value; the continuation lives on
        // `self.walkers` keyed by parent.id (which is preserved on
        // re-enqueue because parent_id chains).
        if !state.walker_exhausted() {
            queue.enqueue(QueueRow {
                id: 0,
                parent_id: Some(parent.id),
                batch_idx: 0,
                path: parent.path.clone(),
                pipe_hash: parent.pipe_hash,
                instance_id: parent.instance_id,
                depth: parent.depth, // SAME depth — re-render FS
                value: parent.value.clone(),
                wake: Wake::Tick { past_tick: 0 }, // park then immediately runnable
                expand_tick: ctx.expand_tick,
                enqueued_at_ns: now_ns(),
            });
            // Re-park the walker state under the NEW row's id.
            // (Or under parent.id if expand reuses the same id for
            // a yield-resume — verify which.)
            self.walkers.lock().unwrap().insert(/* new row id */, state);
        }
    }
}
```

### 3. Expand loop interaction

No change to expand. The mechanism uses existing primitives:
- `Wake::Tick { past_tick: 0 }` parks the FS-resume row until the next global tick. `mem_queue.rs:113-122` already pulls tick-parked rows.
- `splice_into_at` already threads `next_idx` so sibling paths stay unique across multiple FS yields.

### 4. Per-input lifetime

`walkers` map is per-component instance. Cleared when:
- Walker exhausted → state dropped on the last dispatch call (no resume enqueued).
- Cursor `cascade_delete`d (e.g., the FS parent retracted mid-stream) → need a tie-in with `QueueBackend::cascade_delete`, OR rely on parent_id chain — when parent is deleted, the resume row's `parent_id` no longer resolves and FS detects it on next dispatch.

## Sequence of reads/writes

1. expand pulls FS depth=0 batch (one parent row).
2. FS::dispatch consumes the row, creates Walker, takes K=batch_cap entries.
3. FS splices K cursors at depth=1 into queue.
4. FS enqueues yield-row at depth=0 with `Wake::Tick { past_tick: 0 }`.
5. FS stashes Walker in `walkers[parent.id]`.
6. FS returns. Queue has K rows at depth=1 + 1 yield-row at depth=0.
7. expand pulls depth=1 batch (K rows, capped at batch_cap). Runs ast on them. Splices ast outputs at depth=2.
8. expand drains depth=2 (ast outputs through FactWrite). Queue at depth=1 empties.
9. expand pulls depth=0 — sees the yield-row, hands to FS.
10. FS resumes with stashed Walker, takes next K, repeats.

Peak queue residence: K rows at depth=1 + K rows downstream = O(batch_cap).

## Uniqueness conditions

- `batch_idx` continuity across yields: `state.next_idx` thread, already correct in current splice loop.
- `walkers` map lookup MUST find the right state per input parent. Key = parent.id (yield row has `parent_id = original_parent.id`). NOT the yield row's own id — that changes per resume.
- Walker iteration order must be deterministic across resumes (ignore::Walker IS deterministic given the same root).

## Risks / open questions

1. `ignore::Walker` `Send` bound: confirmed in the ignore crate. Storing `Box<dyn Iterator + Send>` works.
2. `cascade_delete` cleanup: if a FS-parent is deleted, the walker remains in `walkers` until GC. Add a sweep when FS sees a yield-row whose parent_id is no longer present.
3. Branch 1 (git rev path, `any_rev` block at `v2_ops.rs:303-400`) currently iterates `walk_via_ls_tree(...).into_iter()` synchronously. Same yield-resume treatment applies; less critical because git-rev walks are usually smaller.
4. Branch 2 (worktree, `warm_slice == None`) is the bench path and gets the fix here.
5. Test: `v4/tests/fs_streaming_budget.rs` — assert peak `queue.depth()` during expand is ≤ 2 × batch_cap for a corpus of N=10*batch_cap files.

## TODO checklist

- [ ] Add `FsWalkState` + `walkers: Mutex<HashMap<QueueId, FsWalkState>>` to `FsComponent`.
- [ ] Refactor branch 2 dispatch loop (`v2_ops.rs:411-650`) into chunk-take + yield-resume.
- [ ] Same for branch 1 (`v2_ops.rs:303-400`).
- [ ] Walker cleanup on cascade_delete / unreachable parent.
- [ ] Stress test: `fs_streaming_budget.rs` — `assert!(max_queue_depth_observed < 2 * batch_cap)`.
- [ ] Bench re-run to confirm no wall-time regression (currently 7s; if chunked-yield serializes ast batches, wall could grow — verify ast still gets `batch_cap`-sized batches each chunk).
- [ ] Backlog if wall regresses: tune `batch_cap` default upward, OR fan multiple yields into the queue at once.

## Critical files

- `v4/src/v2_ops.rs:284-650` (FsComponent dispatch — the change site)
- `v3/crates/effect_runtime/src/v2/mem_queue.rs:113-122` (tick-park pull semantics)
- `v3/crates/effect_runtime/src/v2/flatten.rs:40-54` (splice_into_at with batch_idx_start — already supports streaming splices)
- `v3/crates/effect_runtime/src/v2/queue.rs` (QueueId + cascade_delete contract)
