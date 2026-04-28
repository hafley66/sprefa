//! Relation ops — push and pull primitives over the shared
//! [`RelationStore`](crate::relation_store::RelationStore).
//!
//! tag/rule unification (chat_log/20260426.7): both surfaces lower to ops
//! in this module.
//!
//!   * [`WriteOp`](write::WriteOp) — push a row into a named bag.
//!   * [`QueryOp`](query::QueryOp) — drain bag + perpetually subscribe.
//!   * [`ProbeOp`](probe::ProbeOp) — strict-bound predicate; one row in,
//!     one (or zero) row out. (Step kcl wires this.)
//!   * [`JoinOp`](join::JoinOp) — datalog-style mixed-mode filter+project.
//!     (Step kcl wires this.)
//!
//! `RelationArg` is the shared write-side argument shape: a literal or a
//! capture name. Pull-side ops carry hole names directly.

use std::sync::Arc;

pub mod join;
pub mod probe;
pub mod query;
pub mod write;

pub use join::{JoinOp, JoinSlot};
pub use probe::{ProbeMode, ProbeOp};
pub use query::QueryOp;
pub use write::WriteOp;

/// Write-side argument: literal or capture-by-name. Resolved per cursor at
/// run time by [`materialize_row`].
#[derive(Debug, Clone)]
pub enum RelationArg {
    /// `${X}` — bind value at write time from `cursor.capture(X)`.
    Capture(Arc<str>),
    /// Literal atom / string / number — already a string at lower time.
    Literal(Arc<str>),
}

/// Resolve write-side args to a row of strings. Capture refs read
/// `cursor.capture(name)`; missing captures resolve to empty string (user
/// discipline — no diagnostic at runtime).
pub(crate) fn materialize_row(args: &[RelationArg], c: &crate::_0_cursor::Cursor) -> Vec<Arc<str>> {
    let mut out: Vec<Arc<str>> = Vec::with_capacity(args.len());
    for a in args {
        match a {
            RelationArg::Literal(s) => out.push(s.clone()),
            RelationArg::Capture(name) => match c.capture(name) {
                Some(cap) => {
                    let bytes = cap.bytes(&c.content);
                    let s = std::str::from_utf8(bytes).unwrap_or("");
                    out.push(Arc::from(s));
                }
                None => out.push(Arc::from("")),
            },
        }
    }
    out
}

/// Build one `Arc<[Cursor]>` with one cursor per row. Each cursor inherits
/// `cursor`'s identity and adds `holes[i]` as a `Synthesized` capture
/// holding `row[i]`. Extra row columns and extra holes are silently
/// dropped (zip semantics).
pub(crate) fn rows_to_batch(
    cursor: &crate::_0_cursor::Cursor,
    holes:  &[Arc<str>],
    rows:   Vec<Vec<Arc<str>>>,
) -> Arc<[crate::_0_cursor::Cursor]> {
    let mut out: Vec<crate::_0_cursor::Cursor> = Vec::with_capacity(rows.len());
    for row in rows {
        let mut c = cursor.clone();
        for (hole, value) in holes.iter().zip(row.into_iter()) {
            c.captures
                .push(crate::_0_cursor::Capture::synthesized(hole.clone(), value));
        }
        // sprefa-4iv: set last_bound to the rightmost hole written so the
        // downstream source-op arg-pipe contract (repo/rev/fs consume
        // last_bound) lights up without requiring an explicit per-row
        // bind step.
        if let Some(last) = holes.last() {
            c.last_bound = Some(last.clone());
        }
        out.push(c);
    }
    Arc::from(out)
}
