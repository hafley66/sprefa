//! `WriteOp` — push a row per cursor into a named relation bag.
//!
//! The whole pipe batch is materialized into a single [`WriteEffect`] with
//! one `(name, row)` entry per cursor; the batcher takes the bag lock once,
//! pushes every row, and fires every triggered waiter. Avoids N+1 effect
//! dispatch for cursor-heavy batches and matches the future SQLite write
//! shape (one transaction, one INSERT-many).

use std::sync::Arc;

use effect_runtime::{BoxFuture, RtCtx};

use crate::_0_cursor::Cursor;
use crate::_1_op::Op;
use crate::relation_store::WriteEffect;

use super::{materialize_row, RelationArg};

#[derive(Debug)]
pub struct WriteOp {
    pub name: Arc<str>,
    pub args: Vec<RelationArg>,
}

impl WriteOp {
    pub fn new(name: Arc<str>, args: Vec<RelationArg>) -> Self {
        Self { name, args }
    }
}

impl Op for WriteOp {
    fn name(&self) -> &'static str { "fact" }

    fn pipe<'a>(&'a self, ctx: &'a RtCtx, batch: Arc<[Cursor]>) -> BoxFuture<'a, Arc<[Cursor]>> {
        Box::pin(async move {
            if batch.is_empty() {
                return batch;
            }
            let writes: Vec<(Arc<str>, Vec<Arc<str>>)> = batch
                .iter()
                .map(|c| (self.name.clone(), materialize_row(&self.args, c)))
                .collect();
            let _ = ctx.put(WriteEffect { writes }).await;
            batch
        })
    }
}
