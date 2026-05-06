//! `lower/` — language adaptor (compile-side).
//!
//! Layer shape:
//!   value.rs    — Value (Atom | Pipe). Two variants only.
//!   op_def.rs   — OperatorDef trait + four-slot shape.
//!   ctx.rs      — LowerCtx + LowerError.
//!   registry.rs — Registry + validate_call.
//!   ops.rs      — all OperatorDef wrappers in one file (str, rule, fact_read…).

pub mod ctx;
pub mod op_def;
pub mod ops;
pub mod registry;
pub mod value;

pub use ctx::{LowerCtx, LowerError};
pub use op_def::{
    ArgKind, ArgSig, BlockShape, DslBody, DslInterp, DslShape, OperatorDef,
};
pub use crate::pipeline::{str_pipe, StrConstComponent};
pub use registry::{validate_call, Registry};
pub use value::{run_once_const, Value};

use std::sync::Arc;

pub fn default_registry() -> Registry {
    let mut r = Registry::new();
    r.register(Arc::new(ops::StrDef));
    r.register(Arc::new(ops::RuleDef));
    r.register(Arc::new(ops::FactReadDef));
    r
}
