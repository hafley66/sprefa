//! Flatten a `Node<N>` tree into queue rows.
//!
//! Pure function: takes a parent row's metadata + the render result,
//! produces K children. Done = no rows; Emit = one runnable row;
//! Many = recurse; Yield = one parked row at the parker's depth.
//!
//! Each child's `path` is `parent.path + [batch_idx]`. Roots seeded
//! into `drive` use `path = vec![]` and accumulate from there.

use super::next::Next;
use super::node::Node;
use super::queue::{DriveTick, QueueBackend, QueueRow};
use super::wake::Wake;

/// Splice helper for `Component::dispatch` overrides. Flattens `node`
/// into rows under `parent` at `next_depth`, then enqueues each child.
/// Returns the number of rows enqueued.
///
/// dispatch overrides that compute their own `Node<N>` per row can
/// call this to splice without re-implementing `flatten` + enqueue.
pub fn splice_into<N: Next>(
    parent:     &QueueRow<N>,
    node:       Node<N>,
    next_depth: u32,
    drive_tick: DriveTick,
    queue:      &dyn QueueBackend<N>,
) -> usize {
    let children = flatten(node, parent, next_depth, drive_tick);
    let n = children.len();
    for child in children { queue.enqueue(child); }
    n
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
    node:       Node<N>,
    parent:     &QueueRow<N>,
    next_depth:    u32,
    drive_tick: DriveTick,
) -> Vec<QueueRow<N>> {
    let mut out = Vec::new();
    flatten_into(node, parent, next_depth, drive_tick, &mut out);
    for (i, row) in out.iter_mut().enumerate() {
        row.batch_idx = i as u32;
        row.path = {
            let mut p = parent.path.clone();
            p.push(i as u32);
            p
        };
    }
    out
}

fn flatten_into<N: Next>(
    node:       Node<N>,
    parent:     &QueueRow<N>,
    next_depth:    u32,
    drive_tick: DriveTick,
    out:        &mut Vec<QueueRow<N>>,
) {
    match node {
        Node::Done => { /* no-op */ }

        Node::Emit(value) => {
            out.push(child_row(parent, next_depth, value, Wake::Immediate, drive_tick));
        }

        Node::Many(children) => {
            for c in children {
                flatten_into(c, parent, next_depth, drive_tick, out);
            }
        }

        Node::Yield { value, wake } => {
            // Park at the SAME depth — re-render the same component on wake.
            out.push(child_row(parent, parent.depth, value, wake, drive_tick));
        }
    }
}

fn child_row<N: Next>(
    parent:     &QueueRow<N>,
    depth:         u32,
    value:      std::sync::Arc<N>,
    wake:       Wake,
    drive_tick: DriveTick,
) -> QueueRow<N> {
    QueueRow {
        id:             0,
        parent_id:      Some(parent.id),
        batch_idx:      0,
        path:           Vec::new(),
        pipe_hash:      parent.pipe_hash,
        instance_id:    parent.instance_id,
        depth,
        value,
        wake,
        drive_tick,
        enqueued_at_ns: now_ns(),
    }
}

fn now_ns() -> u64 {
    std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .map(|d| d.as_nanos() as u64)
        .unwrap_or(0)
}
