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

pub use crate::pipeline::{str_pipe, StrConstComponent};
pub use ctx::{LowerCtx, LowerError};
pub use op_def::{ArgKind, ArgSig, BlockShape, DslBody, DslInterp, DslShape, OperatorDef};
pub use registry::{validate_call, Registry};
pub use value::{run_once_const, CallArg, Value};

use std::sync::Arc;

pub fn default_registry() -> Registry {
    let mut r = Registry::new();
    r.register(Arc::new(ops::StrDef));
    r.register(Arc::new(ops::PathDef));
    r.register(Arc::new(ops::RuleDef));
    r.register(Arc::new(ops::FactDef));
    r.register(Arc::new(ops::FactReadDef));
    r.register(Arc::new(ops::FsDef));
    r.register(Arc::new(ops::GlobDef));
    r.register(Arc::new(ops::AstDef));
    r.register(Arc::new(ops::AstYamlDef));
    r.register(Arc::new(ops::JsonDef));
    r.register(Arc::new(ops::ReDef));
    r.register(Arc::new(ops::TermReadDef));
    r.register(Arc::new(ops::TermBindDef));
    r.register(Arc::new(ops::FactWriteDef));
    r.register(Arc::new(ops::TagDef));
    r.register(Arc::new(ops::WhereDef));
    r.register(Arc::new(ops::NextDef));
    r.register(Arc::new(ops::NextQDef));
    r.register(Arc::new(ops::VoidDef));
    r.register(Arc::new(ops::PrintDef));
    r.register(Arc::new(ops::ShDef));
    r.register(Arc::new(ops::ShBangDef));
    r.register(Arc::new(ops::RepoDef));
    r.register(Arc::new(ops::RevDef));
    r.register(Arc::new(ops::SplitDef));
    r.register(Arc::new(ops::SplitLineDef));
    r.register(Arc::new(ops::ReadDef));
    r.register(Arc::new(ops::CommentDef));
    r.register(Arc::new(ops::WriteCursorDef));
    r.register(Arc::new(ops::WriteFileDef));
    r.register(Arc::new(ops::RenderDef));
    r.register(Arc::new(ops::RenderMarkdownDef));
    r.register(Arc::new(ops::RenderDotMarkdownDef));
    r.register(Arc::new(ops::CollectDef));
    r.register(Arc::new(ops::CollectReadyDef));
    r.register(Arc::new(crate::sql::SqlDef));
    #[cfg(feature = "ghcache")]
    r.register(Arc::new(crate::ghcache::GhPrsDef));
    r.register(Arc::new(crate::lsp::LspErrorDef));
    r.register(Arc::new(crate::lsp::LspWarnDef));
    r.register(Arc::new(crate::lsp::LspInfoDef));
    r.register(Arc::new(crate::lsp::LspHintDef));
    r.register(Arc::new(crate::lsp::LspHoverDef));
    r.register(Arc::new(crate::lsp::LspHoverAliasDef));
    r
}
