//! `JoinOp` — datalog-style mixed-mode filter+project over a bag.
//!
//! Mixed `<rel>?(:r, $X, ${Y?})` lowers here. Bound positions filter rows;
//! unbound positions project the matched row column into a fresh capture.
//! One input cursor fans out into N output cursors per matching row (a
//! single batch in → larger batch out).
//!
//! Today: snapshot semantics. The reactive variant (re-fire as later rows
//! arrive) is queued behind cardinality + reactive-rule planning.

use std::sync::Arc;

use effect_runtime::{BoxFuture, RtCtx};

use crate::_0_cursor::{Capture, Cursor};
use crate::_1_op::Op;
use crate::relation_store::RelationStore;

use super::{materialize_row, RelationArg};

/// Per-position spec: bound (matched against the row column) or projected
/// (column value bound to the hole capture name).
#[derive(Debug, Clone)]
pub enum JoinSlot {
    Bound(RelationArg),
    Project(Arc<str>),
}

#[derive(Debug)]
pub struct JoinOp {
    pub name:  Arc<str>,
    pub slots: Vec<JoinSlot>,
}

impl JoinOp {
    pub fn new(name: Arc<str>, slots: Vec<JoinSlot>) -> Self {
        Self { name, slots }
    }
}

impl Op for JoinOp {
    fn name(&self) -> &'static str { "tag" }

    fn pipe<'a>(&'a self, ctx: &'a RtCtx, batch: Arc<[Cursor]>) -> BoxFuture<'a, Arc<[Cursor]>> {
        let store = ctx
            .store::<RelationStore>()
            .expect("RelationStore not registered on RtCtx");
        Box::pin(async move {
            // Snapshot the bag once per batch; downstream cursors share
            // the read.
            let rows = store.rows_snapshot(&self.name);
            let mut out: Vec<Cursor> = Vec::new();
            for cursor in batch.iter() {
                // Resolve bound slots once per cursor; project columns per
                // matching row.
                let bound: Vec<(usize, Arc<str>)> = self
                    .slots
                    .iter()
                    .enumerate()
                    .filter_map(|(i, slot)| match slot {
                        JoinSlot::Bound(arg) => {
                            let v = materialize_row(std::slice::from_ref(arg), cursor)
                                .into_iter()
                                .next()
                                .unwrap_or_else(|| Arc::from(""));
                            Some((i, v))
                        }
                        JoinSlot::Project(_) => None,
                    })
                    .collect();
                for row in rows.iter() {
                    if !bound
                        .iter()
                        .all(|(i, v)| row.get(*i).map(|c| c == v).unwrap_or(false))
                    {
                        continue;
                    }
                    let mut c = cursor.clone();
                    for (i, slot) in self.slots.iter().enumerate() {
                        if let JoinSlot::Project(name) = slot {
                            if let Some(col) = row.get(i) {
                                c.captures
                                    .push(Capture::synthesized(name.clone(), col.clone()));
                            }
                        }
                    }
                    out.push(c);
                }
            }
            Arc::from(out)
        })
    }
}
