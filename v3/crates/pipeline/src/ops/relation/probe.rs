//! `ProbeOp` — strict-bound predicate over a relation bag.
//!
//! All-bound `<rel>?(:r, $X, $Y)` lowers here. One row in, one (or zero)
//! rows out: cursor passes if the bag contains a row exactly matching the
//! key, else dropped.
//!
//! Step kcl wires this. Step rb8/noq lands the type stub so downstream
//! cards (bd5 keyed waiters) can reference the surface.

use std::sync::Arc;

use effect_runtime::{BoxFuture, RtCtx};

use crate::_0_cursor::Cursor;
use crate::_1_op::Op;

use super::RelationArg;

/// Probe semantics: strict snapshot vs reactive (re-fire on later writes).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ProbeMode {
    /// One-shot snapshot probe. Decides at call time, never re-fires.
    Static,
    /// Reactive probe: also re-fires when a matching row arrives later.
    Reactive,
}

#[derive(Debug)]
pub struct ProbeOp {
    pub name: Arc<str>,
    pub key:  Vec<RelationArg>,
    pub mode: ProbeMode,
}

impl ProbeOp {
    pub fn new(name: Arc<str>, key: Vec<RelationArg>, mode: ProbeMode) -> Self {
        Self { name, key, mode }
    }
}

impl Op for ProbeOp {
    fn name(&self) -> &'static str { "tag" }

    fn pipe<'a>(&'a self, _ctx: &'a RtCtx, batch: Arc<[Cursor]>) -> BoxFuture<'a, Arc<[Cursor]>> {
        // Stub: passthrough until kcl wires lookup_by_key + reactive subscribe.
        Box::pin(async move { batch })
    }
}
