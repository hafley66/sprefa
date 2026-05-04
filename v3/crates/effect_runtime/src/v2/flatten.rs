//! Flatten a `Node<N>` tree into queue rows.
//!
//! Pure function: takes a parent row's metadata + the render result,
//! produces K children. Done = no rows; Emit = one runnable row;
//! Many = recurse; Suspense = one parked row.
//!
//! Each child's `path` is `parent.path + [batch_idx]`. Roots seeded
//! into `drive` use `path = vec![]` and accumulate from there.

use super::next::Next;
use super::node::Node;
use super::queue::{DriveTick, QueueRow};
use super::wake::Wake;

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
    next_pc:    u32,
    drive_tick: DriveTick,
) -> Vec<QueueRow<N>> {
    let mut out = Vec::new();
    flatten_into(node, parent, next_pc, drive_tick, &mut out);
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
    next_pc:    u32,
    drive_tick: DriveTick,
    out:        &mut Vec<QueueRow<N>>,
) {
    match node {
        Node::Done => { /* no-op */ }

        Node::Emit(value) => {
            out.push(child_row(parent, next_pc, value, Wake::Immediate, drive_tick));
        }

        Node::Many(children) => {
            for c in children {
                flatten_into(c, parent, next_pc, drive_tick, out);
            }
        }

        Node::Suspense { value, wake } => {
            out.push(child_row(parent, next_pc, value, wake, drive_tick));
        }

        Node::Yield { value, wake } => {
            // Park at the SAME pc — re-render the same component on wake.
            out.push(child_row(parent, parent.pc, value, wake, drive_tick));
        }
    }
}

fn child_row<N: Next>(
    parent:     &QueueRow<N>,
    pc:         u32,
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
        pc,
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
