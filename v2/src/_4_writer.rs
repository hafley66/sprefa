//! Writer trait. All writes bulk-batched. Prefixed `write_` / `rewrite_` / `shell_`.

use std::ops::Range;
use std::sync::Arc;

use bytes::Bytes;
use futures_core::stream::BoxStream;

use crate::_0_types::{FileId, FilePath, RefId, RewriteKind, RowId, RunId, StringId};

// ---------------------------------------------------------------------------
// Table specs
// ---------------------------------------------------------------------------

#[derive(Debug, Clone)]
pub struct RuleTableSpec {
    pub namespace: Arc<str>,
    pub rule:      Arc<str>,
    pub columns:   Vec<ColumnSpec>,
    pub cross_fks: Vec<CrossFk>,
}

#[derive(Debug, Clone)]
pub struct ColumnSpec {
    pub capture_name: Arc<str>,
    pub kind:         ColumnKind,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ColumnKind { StrRef, RefRef, Both }

#[derive(Debug, Clone)]
pub struct CrossFk {
    pub capture_name: Arc<str>,
    pub target_rule:  Arc<str>,
}

// ---------------------------------------------------------------------------
// Writer param structs
// ---------------------------------------------------------------------------

#[derive(Debug, Clone)]
pub struct RefEntry {
    pub file:   FileId,
    pub string: StringId,
    pub span:   Range<u32>,
}

#[derive(Debug, Clone)]
pub struct ExtractionRow {
    pub repo:      Arc<str>,
    pub rev:       Arc<str>,
    pub file:      Option<Arc<str>>,
    pub file_hash: Option<Arc<str>>,
    pub captures:  Vec<(Arc<str>, Option<RefId>, Option<StringId>)>,
    pub fks:       Vec<(Arc<str>, RowId)>,
}

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

#[derive(Debug, Clone)]
pub struct RunVisit {
    pub run_id:      RunId,
    pub op_id:       u64,
    pub path_hash:   u64,
    pub cursor_hash: u64,
}

#[derive(Debug, Clone)]
pub struct ViolationRow {
    pub row_json: Arc<str>,
    pub hash:     u64,
}

#[derive(Debug, Clone)]
pub struct EffectLogRow {
    pub run_id:   RunId,
    pub step:     u32,
    pub parse_id: Arc<str>,
    pub kind:     Arc<str>,
    pub payload:  Arc<str>,
    pub status:   EffectStatus,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum EffectStatus { Ok, Err, Pending }

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

#[derive(Debug, Clone)]
pub struct ShellCall {
    pub program:    Arc<str>,
    pub args:       Vec<Arc<str>>,
    pub cwd:        Option<FilePath>,
    pub stdin:      Option<Bytes>,
    pub timeout_ms: u32,
    pub parse_id:   Arc<str>,
}

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

pub type WResult<T> = Result<T, WriterError>;

#[derive(Debug)]
pub struct WriterError(pub Arc<str>);

pub trait Writer: Send + Sync {
    fn create_rule_table(&self, spec: &RuleTableSpec) -> WResult<()>;

    fn write_strings   (&self, values: &[&str])                           -> WResult<Vec<StringId>>;
    fn write_refs      (&self, entries: &[RefEntry])                      -> WResult<Vec<RefId>>;
    fn write_repos     (&self, names: &[&str])                            -> WResult<()>;
    fn write_revs      (&self, revs: &[(&str, &str)])                     -> WResult<()>;
    fn write_rev_files (&self, entries: &[(&str, &str, FileId)])          -> WResult<()>;
    fn write_rows      (&self, spec: &RuleTableSpec, rows: &[ExtractionRow]) -> WResult<Vec<RowId>>;
    fn write_provenance(&self, rows: &[ProvenanceRow])                    -> WResult<()>;
    fn write_run_visit (&self, rows: &[RunVisit])                         -> WResult<()>;
    fn write_violations(&self, check: &str, rows: &[ViolationRow])        -> WResult<()>;
    fn write_effect_log(&self, rows: &[EffectLogRow])                     -> WResult<()>;

    fn rewrite_files   (&self, edits: &[FileEdit])                        -> WResult<()>;
    fn shell_batch     (&self, calls: &[ShellCall])                       -> BoxStream<'static, Vec<ShellReply>>;

    fn flush(&self) -> BoxStream<'static, ()>;
}
