//! `JoinOp` — datalog-style mixed-mode filter+project over a bag.
//!
//! Mixed `<rel>?(:r, $X, ${Y?})` lowers here: bound positions filter rows,
//! unbound positions bind hole captures projected from the matched rows.
//! Fan-out per matching row.
//!
//! Step kcl wires this. Step noq lands the type stub.

use std::sync::Arc;

use effect_runtime::{BoxFuture, RtCtx};

use crate::_0_cursor::Cursor;
use crate::_1_op::Op;

use super::RelationArg;

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

    fn pipe<'a>(&'a self, _ctx: &'a RtCtx, batch: Arc<[Cursor]>) -> BoxFuture<'a, Arc<[Cursor]>> {
        // Stub: passthrough until kcl wires the snapshot+filter+project pass.
        Box::pin(async move { batch })
    }
}
