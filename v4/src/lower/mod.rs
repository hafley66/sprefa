//! `lower/` — surface that the host-grammar lowerer calls into.
//!
//! Layer shape:
//!   value.rs    — Value (Atom | Pipe). Two variants only.
//!   op_def.rs   — OperatorDef trait + four-slot shape.
//!   ctx.rs      — LowerCtx + LowerError.
//!   registry.rs — Registry + validate_call.
//!   ops/        — concrete defs (str, rule, fact_read).

pub mod ctx;
pub mod op_def;
pub mod ops;
pub mod registry;
pub mod value;

pub use ctx::{LowerCtx, LowerError};
pub use op_def::{
    ArgKind, ArgSig, BlockShape, DslBody, DslInterp, DslShape, OperatorDef,
};
pub use ops::str::{str_pipe, StrConstComponent};
pub use registry::{validate_call, Registry};
pub use value::{run_once_const, Value};

use std::sync::Arc;

pub fn default_registry() -> Registry {
    let mut r = Registry::new();
    r.register(Arc::new(ops::str::StrDef));
    r.register(Arc::new(ops::rule::RuleDef));
    r.register(Arc::new(ops::fact_read::FactReadDef));
    r
}
