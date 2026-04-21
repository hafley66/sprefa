//! Pipeline envelope. Framework owns fan-out semantics.
//!
//! `Seq` threads cursors linearly; `Fork` duplicates input to every arm
//! and concatenates outputs with per-arm tagging. Ops are leaves.
//!
//! PAIN POINT — Fork input duplication cost:
//!
//! v2 buffers the entire input stream into `Vec<Arc<[Cursor]>>` before
//! replay (v2/src/_5_op.rs:217-239). This slice mirrors that with a
//! `Vec<Cursor>` clone-per-arm. Cursor clone is cheap (content and
//! slots are Arc-wrapped, captures and path are shallow Vecs) but this
//! still materializes N × arms cursors in memory. The streaming
//! version (BoxStream<Arc<[Cursor]>> per v2 trait) with broadcast-Fork
//! per project_v2_runner_parallelism is the next slice once the
//! concrete shape stabilizes.
//!
//! PAIN POINT — path tagging happens after `pipe` returns:
//!
//! Ops cannot read the final path their cursors will carry, because the
//! runner appends `PathSeg::Op` *after* receiving Vec<Cursor>. That is
//! intentional per project_v2_path_tagging ("framework owns, ops never
//! touch"), but an op that wants to emit a diagnostic referencing the
//! path would need to read it from its *input* cursor, not from its
//! output. For this slice, ops reference `c.path` from the input side.

use crate::_0_cursor::{Cursor, PathSeg};
use crate::_1_op::Op;
use effect_runtime::{BoxFuture, RtCtx};

pub enum Pipeline {
    Op(Box<dyn Op>),
    Seq(Vec<Pipeline>),
    Fork(Vec<Pipeline>),
}

impl Pipeline {
    pub fn run<'a>(
        &'a self,
        ctx: &'a RtCtx,
        inputs: Vec<Cursor>,
    ) -> BoxFuture<'a, Vec<Cursor>> {
        Box::pin(async move {
            match self {
                Pipeline::Op(op) => {
                    let mut out = Vec::new();
                    for (step, c) in inputs.into_iter().enumerate() {
                        let mut cs = op.pipe(ctx, c).await;
                        for nc in &mut cs {
                            nc.path.push(PathSeg::Op { name: op.name(), step });
                        }
                        out.extend(cs);
                    }
                    out
                }
                Pipeline::Seq(stages) => {
                    let mut cs = inputs;
                    for stage in stages {
                        cs = stage.run(ctx, cs).await;
                    }
                    cs
                }
                Pipeline::Fork(arms) => {
                    let mut out = Vec::new();
                    for (index, arm) in arms.iter().enumerate() {
                        let arm_in: Vec<Cursor> = inputs.clone();
                        let mut cs = arm.run(ctx, arm_in).await;
                        for nc in &mut cs {
                            nc.path.push(PathSeg::ForkArm { index });
                        }
                        out.extend(cs);
                    }
                    out
                }
            }
        })
    }
}
