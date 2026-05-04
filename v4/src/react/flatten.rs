// Flatten a CursorNode tree into queue rows.
//
// Pure function. Takes the consumed parent row's metadata + the node
// returned from render, produces N children rows ready to enqueue.
//
// Suspense / Mount / Effect handling is partial in Phase 3 — they
// panic. Phases 4 and 5 fill them in.

use std::sync::Arc;
use std::sync::atomic::{AtomicU64, Ordering};

use crate::Cursor;
use super::node::CursorNode;
use super::queue::{InstanceId, QueueId, QueueRow};
use super::wake::Wake;

/// Monotonic instance-id source for Mount-allocated subpipe instances.
/// Globally unique per process. Phase 5 will route Mount through here.
pub static NEXT_INSTANCE_ID: AtomicU64 = AtomicU64::new(1);

#[allow(dead_code)]
pub fn next_instance_id() -> InstanceId {
    NEXT_INSTANCE_ID.fetch_add(1, Ordering::SeqCst)
}

/// Flatten one render result into rows ready to enqueue.
///
/// `parent` is the row that was just rendered. Children inherit
/// `pipe_hash` and `instance_id` (they're in the same pipe + mount
/// instance unless explicitly Mounted into a new one).
///
/// `next_pc` is `parent.pc + 1` for normal flow, or held at `parent.pc`
/// for some patterns (none in Phase 3).
pub fn flatten(
    node: CursorNode,
    parent: &QueueRow,
    next_pc: u32,
    drive_tick: u64,
) -> Vec<QueueRow> {
    let mut out = Vec::new();
    flatten_into(node, parent, next_pc, drive_tick, 0, &mut out);
    // Assign batch_idx in order of emission.
    for (i, row) in out.iter_mut().enumerate() {
        row.batch_idx = i as u32;
    }
    out
}

fn flatten_into(
    node: CursorNode,
    parent: &QueueRow,
    next_pc: u32,
    drive_tick: u64,
    _batch_idx_seed: u32,
    out: &mut Vec<QueueRow>,
) {
    match node {
        CursorNode::Done => { /* no-op */ }

        CursorNode::Emit(c) => {
            out.push(child_row(parent, next_pc, c, Wake::Immediate, drive_tick));
        }

        CursorNode::Many(nodes) => {
            for n in nodes {
                flatten_into(n, parent, next_pc, drive_tick, 0, out);
            }
        }

        CursorNode::Suspense { cursor, wake } => {
            out.push(child_row(parent, next_pc, cursor, wake, drive_tick));
        }

        CursorNode::Mount { .. } => {
            panic!("CursorNode::Mount: not implemented until Phase 5");
        }

        CursorNode::Effect { .. } => {
            panic!("CursorNode::Effect: not implemented until Phase 5");
        }
    }
}

fn child_row(
    parent: &QueueRow,
    pc: u32,
    cursor: Arc<Cursor>,
    wake: Wake,
    drive_tick: u64,
) -> QueueRow {
    QueueRow {
        id:             0,                       // backend assigns
        parent_id:      Some(parent.id),
        batch_idx:      0,                       // set by caller
        pipe_hash:      parent.pipe_hash,
        instance_id:    parent.instance_id,
        pc,
        cursor,
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

#[allow(dead_code)]
pub fn parent_id_of(_row: &QueueRow) -> Option<QueueId> {
    None
}
