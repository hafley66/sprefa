//! sprefa::types — thin shim over `spine::types`.
//!
//! All runtime core types (Cursor, Capture, Slots, SprfPath, PathSeg,
//! RunEvent, Tri, CaptureKind, OpEvidence, IDs) now live in the `spine`
//! crate. This module re-exports them under their historical
//! `crate::types::*` paths so existing in-crate callers compile
//! unchanged.
//!
//! Only `CursorExpr` stays here — it references `crate::op::Pipeline`
//! which is still sprefa-local. When phase 2 of the core extraction
//! lands (Op trait + Pipeline → spine), move `CursorExpr` there too.

use std::sync::Arc;

pub use spine::types::*;
pub use sprefa_parse::{ParseSite, ParseSeg};

#[derive(Clone)]
pub struct CursorExpr {
    pub name:     Option<Arc<str>>,
    pub pipeline: crate::op::Pipeline,
}
