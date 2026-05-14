//! Flatten a `Node<N>` tree into queue rows.
//!
//! Pure function: takes a parent row's metadata + the render result,
//! produces K children. Done = no rows; Emit = one runnable row;
//! Many = recurse; Yield = one parked row at the parker's depth.
//!
//! Each child's `path` is `parent.path + [batch_idx]`. Roots seeded
//! into `expand` use `path = vec![]` and accumulate from there.

use super::next::Next;
use super::node::Node;
use super::queue::{ExpandTick, QueueBackend, QueueId, QueueRow};
use super::wake::Wake;

/// Splice helper for `Component::dispatch` overrides. Flattens `node`
/// into rows under `parent` at `next_depth`, then enqueues each child.
/// Returns the number of rows enqueued.
///
/// dispatch overrides that compute their own `Node<N>` per row can
/// call this to splice without re-implementing `flatten` + enqueue.
///
/// PATH INVARIANT: children get `path = parent.path + [batch_idx]` with
/// `batch_idx` starting at 0. A `dispatch` that splices the SAME parent
/// multiple times in one call (e.g. FS streaming flushes) MUST use
/// `splice_into_at` and thread the running offset, otherwise sibling
/// paths collide and any path-keyed downstream (Memoize reconcile)
/// will misidentify children as "the same position re-rendered".
pub fn splice_into<N: Next>(
    parent: &QueueRow<N>,
    node: Node<N>,
    next_depth: u32,
    expand_tick: ExpandTick,
    queue: &dyn QueueBackend<N>,
) -> usize {
    splice_into_at(parent, node, next_depth, expand_tick, queue, 0)
}

/// `splice_into` with explicit starting `batch_idx`. For callers that
/// emit children under one parent across multiple splice calls.
pub fn splice_into_at<N: Next>(
    parent: &QueueRow<N>,
    node: Node<N>,
    next_depth: u32,
    expand_tick: ExpandTick,
    queue: &dyn QueueBackend<N>,
    batch_idx_start: u32,
) -> usize {
    let children = flatten_at(node, parent, next_depth, expand_tick, batch_idx_start);
    let n = children.len();
    for child in children {
        queue.enqueue(child);
    }
    n
}

/// Like `splice_into` but returns the per-child `(content_hash,
/// QueueId)` pairs as enqueued. Used by `Memoize::dispatch` to feed
/// the `PriorChildIndex` for diff-based reconcile.
///
/// Children are flattened and enqueued in the same order as
/// `splice_into`, so paths/batch_idx assignment is identical.
pub fn splice_into_recorded<N: Next>(
    parent: &QueueRow<N>,
    node: Node<N>,
    next_depth: u32,
    expand_tick: ExpandTick,
    queue: &dyn QueueBackend<N>,
) -> Vec<([u8; 32], QueueId)> {
    splice_into_recorded_at(parent, node, next_depth, expand_tick, queue, 0)
}

pub fn splice_into_recorded_at<N: Next>(
    parent: &QueueRow<N>,
    node: Node<N>,
    next_depth: u32,
    expand_tick: ExpandTick,
    queue: &dyn QueueBackend<N>,
    batch_idx_start: u32,
) -> Vec<([u8; 32], QueueId)> {
    let children = flatten_at(node, parent, next_depth, expand_tick, batch_idx_start);
    children
        .into_iter()
        .map(|child| {
            let h = child.value.content_hash();
            let id = queue.enqueue(child);
            (h, id)
        })
        .collect()
}

// PHASE E (deferred): flatten is the natural place to compute the
// per-parent prior-children index. Before returning, mint the
// `Vec<NextKey>` for the new children (via `next_key::compute_key`)
// and stash it on a side store keyed by `parent.id`. The driver's
// reconciliation hook reads from there to multiset-diff against the
// next render's output. Two reasons it's not done today:
//   - the index store doesn't exist yet (would live next to the bus).
//   - parents only render once in the current model, so there's
//     never a prior set to diff against.
pub fn flatten<N: Next>(
    node: Node<N>,
    parent: &QueueRow<N>,
    next_depth: u32,
    expand_tick: ExpandTick,
) -> Vec<QueueRow<N>> {
    flatten_at(node, parent, next_depth, expand_tick, 0)
}

/// `flatten` with explicit starting `batch_idx`. Use when the caller
/// splices a parent's children across multiple calls and needs a
/// continuous path/batch_idx space (FS streaming flushes, paginated
/// source ops). Single-call sites pass 0.
pub fn flatten_at<N: Next>(
    node: Node<N>,
    parent: &QueueRow<N>,
    next_depth: u32,
    expand_tick: ExpandTick,
    batch_idx_start: u32,
) -> Vec<QueueRow<N>> {
    let mut out = Vec::new();
    flatten_into(node, parent, next_depth, expand_tick, &mut out);
    for (i, row) in out.iter_mut().enumerate() {
        let bi = batch_idx_start + i as u32;
        row.batch_idx = bi;
        row.path = {
            let mut p = parent.path.clone();
            p.push(bi);
            p
        };
    }
    out
}

fn flatten_into<N: Next>(
    node: Node<N>,
    parent: &QueueRow<N>,
    next_depth: u32,
    expand_tick: ExpandTick,
    out: &mut Vec<QueueRow<N>>,
) {
    match node {
        Node::Done => { /* no-op */ }

        Node::Emit(value) => {
            out.push(child_row(
                parent,
                next_depth,
                value,
                Wake::Immediate,
                expand_tick,
            ));
        }

        Node::Many(children) => {
            for c in children {
                flatten_into(c, parent, next_depth, expand_tick, out);
            }
        }

        Node::Yield { value, wake } => {
            // Park at the SAME depth — re-render the same component on wake.
            out.push(child_row(parent, parent.depth, value, wake, expand_tick));
        }
    }
}

fn child_row<N: Next>(
    parent: &QueueRow<N>,
    depth: u32,
    value: std::sync::Arc<N>,
    wake: Wake,
    expand_tick: ExpandTick,
) -> QueueRow<N> {
    QueueRow {
        id: 0,
        parent_id: Some(parent.id),
        batch_idx: 0,
        path: Vec::new(),
        pipe_hash: parent.pipe_hash,
        instance_id: parent.instance_id,
        depth,
        value,
        wake,
        expand_tick,
        enqueued_at_ns: now_ns(),
    }
}

fn now_ns() -> u64 {
    std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .map(|d| d.as_nanos() as u64)
        .unwrap_or(0)
}
