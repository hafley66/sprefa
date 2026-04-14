//! Root static types. No I/O, no traits, no behavior.

use std::collections::HashMap;
use std::ops::Range;
use std::path::Path;
use std::sync::Arc;

// ---------------------------------------------------------------------------
// IDs
// ---------------------------------------------------------------------------

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub struct RunId(pub u64);

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub struct OpId(pub u64);

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub struct RowId(pub u64);

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub struct FileId(pub u64);

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub struct StringId(pub u64);

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub struct RefId(pub u64);

// ---------------------------------------------------------------------------
// Severity, Span, FilePath
// ---------------------------------------------------------------------------

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum Severity {
    Error,
    Warn,
    Info,
    Hint,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub struct Span {
    pub start: u32,
    pub end: u32,
}

#[derive(Debug, Clone, PartialEq, Eq, Hash)]
pub struct FilePath(pub Arc<Path>);

// ---------------------------------------------------------------------------
// ParseSite — compile-time stable coordinate
// ---------------------------------------------------------------------------

#[derive(Debug, Clone, PartialEq, Eq, Hash)]
pub struct ParseSite {
    pub file:       Arc<Path>,
    pub path:       Arc<[ParseSeg]>,
    pub byte_range: Range<usize>,
}

#[derive(Debug, Clone, PartialEq, Eq, Hash)]
pub enum ParseSeg {
    Top        { index: u16 },
    BraceChild { index: u16 },
    ParenChild { index: u16 },
    PatternLeaf{ key: Arc<str> },
}

// ---------------------------------------------------------------------------
// SprfPath — runtime per-cursor trail
// ---------------------------------------------------------------------------

#[derive(Debug, Clone)]
pub struct SprfPath(pub Arc<[PathSeg]>);

#[derive(Debug, Clone)]
pub enum PathSeg {
    Op       { name: Arc<str>, parse_site: Arc<ParseSite>, step: u16 },
    Named    { name: Arc<str>, key: Arc<str>, parse_site: Arc<ParseSite> },
    ForkArm  { index: u16, parse_site: Arc<ParseSite> },
    SwitchArm{ pat: Arc<str>, parse_site: Arc<ParseSite> },
    LeafArm  { key: Arc<str>, parse_site: Arc<ParseSite> },
    Iter     { index: u64 },
}

// ---------------------------------------------------------------------------
// Capture + Cursor
// ---------------------------------------------------------------------------

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Tri { Claimed, Verified, Missing }

#[derive(Debug, Clone)]
pub struct Capture {
    pub value:        Arc<str>,
    pub ref_id:       Option<RefId>,
    pub scan_pointer: Option<Arc<str>>,
    pub verified:     Tri,
}

impl Capture {
    pub fn new(value: Arc<str>) -> Self {
        Self { value, ref_id: None, scan_pointer: None, verified: Tri::Claimed }
    }
    pub fn with_ref_id(mut self, r: RefId) -> Self {
        self.ref_id = Some(r);
        self
    }
    pub fn with_scan(mut self, p: Arc<str>, v: Tri) -> Self {
        self.scan_pointer = Some(p);
        self.verified = v;
        self
    }
}

#[derive(Debug, Clone)]
pub struct Cursor {
    pub run_id:   RunId,
    pub repo:     Arc<str>,
    pub rev:      Arc<str>,
    pub fs:       Option<FilePath>,
    pub captures: HashMap<Arc<str>, Capture>,
    pub fks:      HashMap<Arc<str>, RowId>,
    pub path:     SprfPath,
    pub evidence: Vec<OpEvidence>,
    pub content:  Option<Arc<bytes::Bytes>>,
}

/// Per-op match record on a cursor. Framework-appended telemetry for LSP;
/// runtime execution ignores it. Append-only, leaf-first, one entry per
/// op the cursor passed through that opted in via `Op::witness`.
#[derive(Debug, Clone)]
pub struct OpEvidence {
    pub op_name:    &'static str,
    pub parse_site: Arc<ParseSite>,
    pub matched:    Arc<str>,
    pub capture:    Option<Arc<str>>,
}

// ---------------------------------------------------------------------------
// RunCtx — a slice of runtime identity, attached to diagnostics
// ---------------------------------------------------------------------------

#[derive(Debug, Clone)]
pub struct RunCtx {
    pub run_id: RunId,
    pub op_id:  OpId,
    pub path:   SprfPath,
}

// ---------------------------------------------------------------------------
// RunEvent — sole Runner output surface
// ---------------------------------------------------------------------------

pub enum RunEvent {
    RunStarted    { run_id: RunId, config_hash: u64 },
    RuleSkipped   { rule: Arc<str>, reason: SkipReason },
    RuleStarted   { rule: Arc<str>, parse_site: Arc<ParseSite> },
    CursorIn      { rule: Arc<str>, cursor_hash: u64 },
    CursorOut     { rule: Arc<str>, cursor_hash: u64, rows_written: u32 },
    DiagBatch     { diagnostics: Vec<Box<dyn crate::Diagnostic>> },
    FlushStarted  { run_id: RunId, table_count: u32 },
    FlushCompleted{ run_id: RunId, rows: u64, bytes: u64, elapsed_ms: u32 },
    Backpressure  { rule: Arc<str>, lagged: u32 },
    RunCompleted  { run_id: RunId, status: RunStatus },
}

#[derive(Debug, Clone)]
pub enum SkipReason {
    CacheHit,
    ConfigFilter,
    UpstreamEmpty,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum RunStatus {
    Ok,
    HadErrors,
    Aborted,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum RewriteKind {
    Insert,
    Replace,
    Delete,
}
