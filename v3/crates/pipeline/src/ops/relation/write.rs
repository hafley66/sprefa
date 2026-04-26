//! `WriteOp` — push a row into a named relation bag.
//!
//! Per cursor: materialize args via captures + literals, dispatch one
//! [`WriteEffect`](crate::relation_store::WriteEffect). The cursor passes
//! through unchanged (writes are observation-side).

use std::sync::Arc;

use effect_runtime::{BoxFuture, RtCtx};

use crate::_0_cursor::Cursor;
use crate::_1_op::{per_cursor, Op};
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
    fn name(&self) -> &'static str { "tag" }

    fn pipe<'a>(&'a self, ctx: &'a RtCtx, batch: Arc<[Cursor]>) -> BoxFuture<'a, Arc<[Cursor]>> {
        Box::pin(per_cursor(batch, move |c| async move {
            let row = materialize_row(&self.args, &c);
            let _ = ctx
                .put(WriteEffect {
                    name: self.name.clone(),
                    row,
                })
                .await;
            vec![c]
        }))
    }
}
