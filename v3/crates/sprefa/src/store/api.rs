//! `Store` trait — single persistence + mutation-cache interface.
//!
//! # Role
//! The runner calls `flush_batch` at end-of-run; ops call `effect_status`
//! / `record_effect` via `await_approval`; discovery calls `files_scanned`.
//! One impl (SqliteStore, Phase 2) backs all of these.
//!
//! # Ownership + lifecycle
//! `Arc<dyn Store>` threads through `OpCtx`, `DocSession`, and the
//! evaluator. Lives for the whole process; droppable on DocSession drop.
//!
//! # Who mutates
//! `flush_batch`, `record_effect`, and `register_expr_schema` are writes.
//! All other methods are reads. Impls own their own concurrency.
//!
//! # Failure modes
//! Every method returns `Result<_, StoreErr>`. Callers bubble `StoreErr`
//! into `RunEvent::Diag` via the `Diagnostic` impl on `StoreErr`.

use std::collections::HashSet;
use std::sync::Arc;

use async_trait::async_trait;

use crate::types::FilePath;
use crate::mutations::MutationEffect;

use super::types::*;

#[async_trait]
pub trait Store: Send + Sync {
    async fn init(&self) -> Result<(), StoreErr>;
    async fn register_expr_schema(&self, spec: ExprTableSpec) -> Result<(), StoreErr>;
    async fn flush_batch(&self, b: Batch) -> Result<(), StoreErr>;
    async fn query_expr(&self, expr_name: &str, w: Where) -> Result<Vec<Row>, StoreErr>;
    async fn files_scanned(
        &self,
        repo:         &str,
        rev:          &str,
        scanner_hash: &str,
    ) -> Result<HashSet<(FilePath, ContentHash)>, StoreErr>;
    async fn effect_status(&self, e: &dyn MutationEffect) -> Result<EffectStatus, StoreErr>;
    async fn record_effect(
        &self,
        e: &dyn MutationEffect,
        o: EffectOutcome,
    ) -> Result<(), StoreErr>;
}

/// Schema declaration for one named cursor-expr's output table.
/// `schema_hash` pins the column shape; `extract_hash` pins the
/// extraction logic (rule body + ops used). A drift in either forces
/// a DROP + rebuild at register time.
pub struct ExprTableSpec {
    pub expr_name:    Arc<str>,
    pub namespace:    Option<Arc<str>>,
    pub captures:     Vec<CaptureColumn>,
    pub schema_hash:  Arc<str>,
    pub extract_hash: Arc<str>,
}

/// One column of an `ExprTableSpec`. `scan_pointer` names the column
/// class for cross-rule joins — see `_scan_pointer_runtime` notes.
pub struct CaptureColumn {
    pub name:         Arc<str>,
    pub scan_pointer: Option<Arc<str>>,
}

/// `Store` impl that persists nothing. Every write is a no-op; every read
/// returns empty. Used by DocSession (LSP + hover tests) where per-keystroke
/// SQLite flushes would add I/O with no payoff. SqliteStore backs the CLI.
pub struct NoopStore;

impl NoopStore {
    pub fn new() -> Self { Self }
}

impl Default for NoopStore {
    fn default() -> Self { Self::new() }
}

#[async_trait]
impl Store for NoopStore {
    async fn init(&self) -> Result<(), StoreErr> { Ok(()) }
    async fn register_expr_schema(&self, _spec: ExprTableSpec) -> Result<(), StoreErr> { Ok(()) }
    async fn flush_batch(&self, _b: Batch) -> Result<(), StoreErr> { Ok(()) }
    async fn query_expr(&self, _expr_name: &str, _w: Where) -> Result<Vec<Row>, StoreErr> {
        Ok(Vec::new())
    }
    async fn files_scanned(
        &self,
        _repo:         &str,
        _rev:          &str,
        _scanner_hash: &str,
    ) -> Result<HashSet<(FilePath, ContentHash)>, StoreErr> {
        Ok(HashSet::new())
    }
    async fn effect_status(&self, _e: &dyn MutationEffect) -> Result<EffectStatus, StoreErr> {
        Ok(EffectStatus::Emit)
    }
    async fn record_effect(
        &self,
        _e: &dyn MutationEffect,
        _o: EffectOutcome,
    ) -> Result<(), StoreErr> { Ok(()) }
}
