//! pipeline — Op trait, Cursor, Pipeline envelope.
//!
//! Consumes `effect_runtime` as-is. No other deps.
//!
//! Layer map:
//!   - _0_cursor: the payload; slots, captures, path, content+range
//!   - _1_op:     Op trait (object-safe); framework-facing surface
//!   - _2_pipeline: Pipeline enum + runner with path tagging
//!
//! See PAIN_POINTS.md for the friction discovered while building this
//! slice, ranked by severity for the next move.

#[allow(non_snake_case)]
pub mod _0_cursor;
#[allow(non_snake_case)]
pub mod _1_op;
#[allow(non_snake_case)]
pub mod _2_pipeline;
pub mod lower;
pub mod ops;

pub use _0_cursor::{Capture, Cursor, PathSeg, SprfPath};
pub use _1_op::Op;
pub use _2_pipeline::Pipeline;
pub use ops::{CaptureWriteOp, VoidOp};
