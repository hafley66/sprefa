//! Writer trait. All writes bulk-batched. Prefixed `write_` / `rewrite_` / `shell_`.

use std::ops::Range;
use std::sync::Arc;

use bytes::Bytes;
use futures_core::stream::BoxStream;

use crate::_0_types::{FileId, FilePath, RefId, RewriteKind, RowId, RunId, StringId};

// ---------------------------------------------------------------------------
// Table specs
// ---------------------------------------------------------------------------

/// Schema for the extraction table a rule writes into. One `RuleTableSpec`
/// per named rule, built from the rule's captures list at
/// `ops::_0_rule::pipe()`. The writer uses it to create the table on
/// first call and to validate row shape on insert.
#[derive(Debug, Clone)]
pub struct RuleTableSpec {
    pub namespace: Arc<str>,
    pub rule:      Arc<str>,
    pub columns:   Vec<ColumnSpec>,
    pub cross_fks: Vec<CrossFk>,
}

/// One column of a `RuleTableSpec`. `capture_name` is the `$NAME` from
/// the rule body; `kind` determines the SQL storage shape.
#[derive(Debug, Clone)]
pub struct ColumnSpec {
    pub capture_name: Arc<str>,
    pub kind:         ColumnKind,
}

/// Storage shape of a captured value. `StrRef` = string interning only,
/// `RefRef` = file+span only, `Both` = both (the column can serve as an
/// anchor for xrefs and as a value for equality joins).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ColumnKind { StrRef, RefRef, Both }

/// Foreign-key edge from this rule's column to another rule's table.
/// Emitted when a capture resolves to a row in the target rule.
#[derive(Debug, Clone)]
pub struct CrossFk {
    pub capture_name: Arc<str>,
    pub target_rule:  Arc<str>,
}

// ---------------------------------------------------------------------------
// Writer param structs
// ---------------------------------------------------------------------------

/// A string occurrence anchored at (file, span). Written by
/// `Writer::write_refs`; rows point to interned strings via `StringId`.
#[derive(Debug, Clone)]
pub struct RefEntry {
    pub file:   FileId,
    pub string: StringId,
    pub span:   Range<u32>,
}

/// One row written into a rule's extraction table. `captures` stores
/// per-column pre-interned ids; `fks` stores cross-rule joins resolved
/// at emit time.
#[derive(Debug, Clone)]
pub struct ExtractionRow {
    pub repo:      Arc<str>,
    pub rev:       Arc<str>,
    pub file:      Option<Arc<str>>,
    pub file_hash: Option<Arc<str>>,
    pub captures:  Vec<(Arc<str>, Option<RefId>, Option<StringId>)>,
    pub fks:       Vec<(Arc<str>, RowId)>,
}

/// A provenance edge: "row in `source_rule.source_column` caused us to
/// select `(selected_repo, selected_rev)` at step `step_index` of `run_id`."
/// Surfaces in hover and debug traces.
#[derive(Debug, Clone)]
pub struct ProvenanceRow {
    pub run_id:          RunId,
    pub step_index:      u32,
    pub source_rule:     Arc<str>,
    pub source_column:   Arc<str>,
    pub source_ref_id:   Option<RefId>,
    pub source_parse_id: Arc<str>,
    pub selected_repo:   Arc<str>,
    pub selected_rev:    Arc<str>,
    pub reason:          Arc<str>,
}

/// Record of a cursor visit within a run. Lets `Reader::run_visited`
/// short-circuit stable-subtree re-walks on later passes.
#[derive(Debug, Clone)]
pub struct RunVisit {
    pub run_id:      RunId,
    pub op_id:       u64,
    pub path_hash:   u64,
    pub cursor_hash: u64,
}

/// A single assertion failure row. `row_json` is the payload the check
/// rule emitted; `hash` dedupes repeats across runs.
#[derive(Debug, Clone)]
pub struct ViolationRow {
    pub row_json: Arc<str>,
    pub hash:     u64,
}

/// Log entry for one mutation effect. Persisted regardless of approval
/// so the mutation cache can reason about Skip / Stale / Emit on later
/// runs (see `store::EffectStatus`).
#[derive(Debug, Clone)]
pub struct EffectLogRow {
    pub run_id:   RunId,
    pub step:     u32,
    pub parse_id: Arc<str>,
    pub kind:     Arc<str>,
    pub payload:  Arc<str>,
    pub status:   EffectStatus,
}

/// Terminal status of a logged effect. `Pending` means approval was
/// requested but no ack landed before the run drained.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum EffectStatus { Ok, Err, Pending }

/// One file edit queued by a write-op. Applied as a batch by
/// `Writer::rewrite_files` after pipeline drain.
#[derive(Debug, Clone)]
pub struct FileEdit {
    pub repo:        Arc<str>,
    pub rev:         Arc<str>,
    pub fs:          FilePath,
    pub byte_range:  Range<usize>,
    pub replacement: Bytes,
    pub kind:        RewriteKind,
    pub parse_id:    Arc<str>,
}

/// One shell invocation queued by a write-op. Applied as a batch by
/// `Writer::shell_batch`; replies are returned in call order.
#[derive(Debug, Clone)]
pub struct ShellCall {
    pub program:    Arc<str>,
    pub args:       Vec<Arc<str>>,
    pub cwd:        Option<FilePath>,
    pub stdin:      Option<Bytes>,
    pub timeout_ms: u32,
    pub parse_id:   Arc<str>,
}

/// Result of one `ShellCall`.
#[derive(Debug, Clone)]
pub struct ShellReply {
    pub exit_code:  i32,
    pub stdout:     Bytes,
    pub stderr:     Bytes,
    pub elapsed_ms: u32,
}

// ---------------------------------------------------------------------------
// Trait
// ---------------------------------------------------------------------------

/// Writer-side error channel. Single-arm today; Phase 2 may split once
/// the sqlite writer lands with its own error set.
pub type WResult<T> = Result<T, WriterError>;

/// Generic writer error; string payload carries the backend message.
#[derive(Debug)]
pub struct WriterError(pub Arc<str>);

/// Write-side boundary for the pipeline.
///
/// All row-level methods take slices and commit in one transaction per
/// call where the backend supports it. The sole concrete impl today is
/// `MemWriter` (tests, LSP hover); Phase 2 adds a sqlite-backed writer.
///
/// ## Section split
///
/// - `write_*` methods: persistent row writes against the per-expr
///   table set. Additive: replays are filtered by `schema_hash`.
/// - `rewrite_files` / `shell_batch`: deferred effects flushed after
///   pipeline drain (Phase 2).
/// - `flush`: terminator — backend commits and readies for the next run.
pub trait Writer: Send + Sync {
    /// Create (or validate) the table backing `spec`. Idempotent.
    fn create_rule_table(&self, spec: &RuleTableSpec) -> WResult<()>;

    /// Intern `values`; returned ids align 1:1 with input slice order.
    fn write_strings   (&self, values: &[&str])                           -> WResult<Vec<StringId>>;
    /// Persist string occurrences (file, span, string-id).
    fn write_refs      (&self, entries: &[RefEntry])                      -> WResult<Vec<RefId>>;
    /// Register repo slugs with the writer's repo catalog.
    fn write_repos     (&self, names: &[&str])                            -> WResult<()>;
    /// Register (repo, rev) pairs with the writer's rev catalog.
    fn write_revs      (&self, revs: &[(&str, &str)])                     -> WResult<()>;
    /// Associate `FileId` with (repo, rev) triples.
    fn write_rev_files (&self, entries: &[(&str, &str, FileId)])          -> WResult<()>;
    /// Insert extraction rows into the rule's table. Returned ids align
    /// 1:1 with `rows` order. Caller batches, see `ops::_0_rule::pipe`.
    fn write_rows      (&self, spec: &RuleTableSpec, rows: &[ExtractionRow]) -> WResult<Vec<RowId>>;
    /// Log per-step provenance edges.
    fn write_provenance(&self, rows: &[ProvenanceRow])                    -> WResult<()>;
    /// Log cursor visits.
    fn write_run_visit (&self, rows: &[RunVisit])                         -> WResult<()>;
    /// Log assertion failures for a named check.
    fn write_violations(&self, check: &str, rows: &[ViolationRow])        -> WResult<()>;
    /// Log mutation effects and their terminal status.
    fn write_effect_log(&self, rows: &[EffectLogRow])                     -> WResult<()>;

    /// Apply file edits as a batch. Phase 2 surface.
    fn rewrite_files   (&self, edits: &[FileEdit])                        -> WResult<()>;
    /// Run a batch of shell invocations; replies align with `calls` order.
    fn shell_batch     (&self, calls: &[ShellCall])                       -> BoxStream<'static, Vec<ShellReply>>;

    /// Terminator. Backends that buffer commit here; others no-op.
    fn flush(&self) -> BoxStream<'static, ()>;
}
