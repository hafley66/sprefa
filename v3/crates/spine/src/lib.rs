//! spine — semantic core of sprefa v3.
//!
//! What lives here: the types and traits that ops construct, return, and
//! implement against. Nothing that smells like runtime glue (registry,
//! store impls, effect wiring, LSP, CLI).
//!
//! Dependency posture:
//!   spine  depends on  sprefa_parse  (for ParseSite / AST re-exports)
//!                      effect_runtime (RtCtx surface only)
//!   sprefa depends on  spine + sprefa_parse + the world.
//!
//! Why: ops written against spine can live in their own crates without
//! pulling sqlite, git2, tokio-tungstenite, tower-lsp, etc. Phase 1 of
//! the core extraction moves the data+trait tier; phase 2 will move the
//! `Op` trait family (needs an OpCtx/registry abstraction first).
//!
//! What phase 1 covers:
//!   types.rs         Cursor, Capture, Slots, SprfPath, PathSeg, RunEvent
//!   diagnostic.rs    Diagnostic + Renderer traits
//!   config.rs        Config + RuntimeConfig
//!   result_store.rs  ResultStore (shared rule-result data structure)
//!   pattern.rs       CompiledPattern (reader wants it)
//!   reader.rs        Reader trait + ParsedTree / ScanKind / ScanCombo
//!   writer.rs        Writer trait + row types + WResult
//!   mutations.rs     MutationEffect trait + request types
//!
//! What phase 2 will add (staged in sprefa today):
//!   op.rs            Op trait, Pipeline enum, OpCtx, ProgramCtx, RuleHandle
//!   (requires abstracting OperatorRegistry + Store traits into spine so
//!    OpCtx stops reaching into sprefa)

pub mod types;
pub mod diagnostic;
pub mod config;
pub mod result_store;
pub mod pattern;
pub mod reader;
pub mod writer;
pub mod mutations;

// Crate-root re-exports so `spine::Cursor` etc. resolve without the
// module path, and in-crate `crate::Reader` paths work in moved files.
pub use types::*;
pub use diagnostic::{Diagnostic, Renderer};
pub use config::{Config, ConfigDiff, RuntimeConfig};
pub use result_store::{ResultStore, RuleResult, CaptureMap};
pub use pattern::{CompiledPattern, PatternMatcher, Segment, compile_pattern, compile_patterns};
pub use reader::{Reader, ParserKind, ParsedTree, ScanKind, ScanCombo, CrossRefHit, ViolationEntry};
// NOTE: `writer::EffectStatus` (log-row disposition Ok/Err/Pending) and
// `mutations::EffectStatus` (cache disposition Skip/Emit/Stale) share a
// name; both stay accessible via their module path. The crate-root
// shortcut re-exports the writer one to preserve historical paths.
pub use writer::{
    Writer, WResult, WriterError,
    RuleTableSpec, ColumnSpec, ColumnKind, CrossFk,
    RefEntry, ExtractionRow, ProvenanceRow, RunVisit, ViolationRow, EffectLogRow,
    EffectStatus, FileEdit, ShellCall, ShellReply,
};
pub use mutations::{
    MutationEffect, MutationHandler, MutationRequest, MutationScope,
    Approve, Cancelled, EffectErr,
    EffectOutcome, EffectResult,
};
