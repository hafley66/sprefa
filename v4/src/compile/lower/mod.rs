//! `lower/` — language adaptor (compile-side).
//!
//! Layer shape:
//!   value.rs    — Value (Atom | Pipe). Two variants only.
//!   op_def.rs   — OperatorDef trait + four-slot shape.
//!   ctx.rs      — LowerCtx + LowerError.
//!   registry.rs — Registry + validate_call.
//!   ops.rs      — all OperatorDef wrappers in one file (str, rule, fact_read…).

pub mod ctx;
pub mod liftable;
pub mod op_def;
pub mod ops;
pub mod registry;
pub mod value;
pub mod where_eval;

pub use liftable::{stream_noop, Col, IdKind, Liftable, Op, StreamFn, TermBind};

use rusqlite::types::ValueRef;

/// Compute the canonical 32-byte blake3 id for a row, identical to the
/// `sprf_blake3_id` SQL UDF. Exposed for tests that need to assert
/// bytewise equality between the Rust and SQL paths.
pub fn blake3_id_for_test(table: &str, cols: &[i64]) -> Vec<u8> {
    let mut h = blake3::Hasher::new();
    h.update(table.as_bytes());
    h.update(b"\0");
    for v in cols {
        h.update(&v.to_le_bytes());
        h.update(b"\0");
    }
    h.finalize().as_bytes().to_vec()
}

/// Same routine the UDF uses internally — accepts the raw `ValueRef`
/// stream the SQLite callback hands us so integer / blob / text inputs
/// hash identically across the surface (`Rust → bytes`,
/// `SQL → bytes`).
pub fn blake3_id_from_sql_args(table: &str, args: &[ValueRef<'_>]) -> Vec<u8> {
    let mut h = blake3::Hasher::new();
    h.update(table.as_bytes());
    h.update(b"\0");
    for v in args {
        match v {
            ValueRef::Null => h.update(b"\0"),
            ValueRef::Integer(i) => h.update(&i.to_le_bytes()),
            ValueRef::Real(f) => h.update(&f.to_le_bytes()),
            ValueRef::Text(s) => h.update(s),
            ValueRef::Blob(b) => h.update(b),
        };
        h.update(b"\0");
    }
    h.finalize().as_bytes().to_vec()
}

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
    r.register(Arc::new(crate::lsp::ExpectZeroDef));
    r.register(Arc::new(crate::lsp::ExpectMatchDef));
    r
}
