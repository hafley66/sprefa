//! Store value types — batches, rows, query filters, effect outcomes, errors.
//!
//! # Role
//! Plain data shapes shared between the `Store` trait and its callers.
//! No I/O, no traits-with-behavior, no sqlx dependency yet.
//!
//! # Ownership + lifecycle
//! Values are constructed by the runner (Batch, RowInsert) and by ops
//! (MutationEffect fingerprints flow through `EffectOutcome`). They are
//! consumed by the `Store` impl and dropped at the end of the call.
//!
//! # Who mutates
//! All types here are owned + passed by value. No interior mutability.
//!
//! # Failure modes
//! `StoreErr` is the single error channel for every `Store` method.

use std::collections::HashSet;
use std::ops::Range;
use std::sync::Arc;

use chrono::{DateTime, Utc};

use crate::types::{Capture, FilePath, OpEvidence};
use crate::types::Severity;
use crate::diagnostic::{Diagnostic, Renderer};
use crate::types::ParseSite;

/// Stable hash of a blob's content. Used by `files_scanned` to decide
/// whether a (repo, rev, fs) triple needs re-scanning.
#[derive(Debug, Clone, PartialEq, Eq, Hash)]
pub struct ContentHash(pub Arc<str>);

/// Top-level unit passed to `Store::flush_batch` at end-of-run.
/// `scanner_hash` identifies the rule set; `per_expr` holds one batch
/// per named cursor-expr that ran.
#[derive(Debug, Clone)]
pub struct Batch {
    pub scanner_hash: Arc<str>,
    pub per_expr:     Vec<ExprBatch>,
}

/// Rows produced by one named cursor-expr in a run.
#[derive(Debug, Clone)]
pub struct ExprBatch {
    pub expr_name: Arc<str>,
    pub rows:      Vec<RowInsert>,
}

/// One cursor-level row ready to persist. `captures` holds the raw
/// `Capture` values (with scan pointers); `evidence` holds the op
/// sequence that produced the row.
#[derive(Debug, Clone)]
pub struct RowInsert {
    pub repo:     Arc<str>,
    pub rev:      Arc<str>,
    pub file:     FilePath,
    pub captures: std::collections::HashMap<Arc<str>, Capture>,
    pub evidence: Arc<[OpEvidence]>,
}

/// Query filter for `Store::query_expr`. `captures` is a set of
/// (name, value) equality predicates ANDed together; `limit` caps rows.
#[derive(Debug, Clone)]
pub struct Where {
    pub repo:     Option<Arc<str>>,
    pub rev:      Option<Arc<str>>,
    pub captures: Vec<(Arc<str>, Arc<str>)>,
    pub limit:    usize,
}

/// One row returned by `Store::query_expr`. Captures map name → value
/// string; `file` + `span` pin the row's primary anchor.
#[derive(Debug, Clone)]
pub struct Row {
    pub captures: std::collections::HashMap<Arc<str>, Arc<str>>,
    pub file:     FilePath,
    pub span:     Range<usize>,
}

// Effect cache result types live in `spine::mutations` — ops construct
// `EffectOutcome` inside `MutationEffect::apply`, the sqlite Store in
// this crate queries/persists them via `effect_status`/`record_effect`.
pub use spine::mutations::{EffectStatus, EffectOutcome, EffectResult};

/// Error channel for every `Store` method.
#[derive(Debug)]
pub enum StoreErr {
    // Phase 2: swap to sqlx::Error
    Sql(String),
    UnknownExpr,
    // Phase 2: swap to serde_json::Error
    Serde(String),
}

/// A fallback site used by `StoreErr`'s `Diagnostic` impl. Store errors do
/// not have a natural `.sprf` source coordinate; Phase 2 will plumb the
/// originating call site through. For now, a synthetic zero-range site
/// keeps the trait satisfiable.
fn synthetic_site() -> &'static ParseSite {
    use std::sync::OnceLock;
    static SITE: OnceLock<ParseSite> = OnceLock::new();
    SITE.get_or_init(|| ParseSite {
        file:       Arc::from(std::path::Path::new("")),
        path:       Arc::from(Vec::<crate::types::ParseSeg>::new().into_boxed_slice()),
        byte_range: 0..0,
    })
}

impl Diagnostic for StoreErr {
    fn code(&self) -> &str {
        match self {
            StoreErr::Sql(_)      => "store/sql",
            StoreErr::UnknownExpr => "store/unknown-expr",
            StoreErr::Serde(_)    => "store/serde",
        }
    }
    fn severity(&self) -> Severity { Severity::Error }
    fn primary(&self) -> &ParseSite { synthetic_site() }
    fn render(&self, out: &mut dyn Renderer) {
        let msg = match self {
            StoreErr::Sql(s)      => format!("sql error: {s}"),
            StoreErr::UnknownExpr => "unknown cursor expression".to_string(),
            StoreErr::Serde(s)    => format!("serde error: {s}"),
        };
        out.header(self.code(), self.severity(), &msg);
    }
}

/// Keep the imports used in types alive for downstream consumers.
#[allow(dead_code)]
fn _hashset_used(_: HashSet<(FilePath, ContentHash)>) {}
