//! `cst` — sprf-blind v2 runtime for tree-sitter + LSP + path-addressable
//! multi-tree composition.
//!
//! Layout:
//!   path     — Path, PathItem, PathBuilder, lib-shipped items
//!   doc      — Doc, DocId
//!   injected — Injected sub-tree
//!   locate   — locate(byte) ↔ resolve(path)
//!   dsl      — Dsl, Compiled, CaptureRow, CaptureSink
//!   diag     — Diag, Severity, DiagSink, BufferSink
//!   lsp      — body-pure provider trait + position math + shift helpers
//!   store    — Store trait + MemStore
//!   build    — grammar.js compile helper for build.rs (stub)

pub mod build;
pub mod diag;
pub mod dsls;
pub mod doc;
pub mod dsl;
pub mod injected;
pub mod locate;
pub mod lsp;
pub mod path;
pub mod store;

pub use diag::{BufferSink, Diag, DiagSink, LogSink, Severity, SilentSink};
pub use doc::{Doc, DocId};
pub use dsl::{CaptureKind, CaptureRow, CaptureSink, Compiled, Dsl, VecCaptureSink};
pub use injected::{parse_body, Injected};
pub use locate::{locate, resolve, Located};
pub use lsp::{DslBodyLsp, SemanticToken};
pub use path::{ts_default_emit, Path, PathBuilder, PathItem, PathKey};
pub use store::{MemStore, Store};
