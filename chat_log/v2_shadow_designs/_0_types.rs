//! Core types for v2 operator system.
//!
//! DESIGN NOTES (distilled from session):
//! - Operators are pipe transforms: Stream<Cursor> -> Stream<Cursor>
//! - Store provides FREE READS (list_files, read_file, query_refs)
//! - Operators return EFFECTS only for mutations (shell, writes, refactors)
//! - Discovery is first-class: captures have [repo], [rev], [repo.norm], [rev.norm] annotations
//! - Cross-refs are grounded queries: ${rule.$VAR} reads from Store
//! - Diagnostics have EXACT byte ranges for LSP
//! - Pure operators have Effect = ()
//! - Box<dyn> everywhere - optimize later

use std::collections::HashMap;
use std::ops::Range;
use std::path::PathBuf;

/// Location in source file (sprf or target).
#[derive(Debug, Clone)]
pub struct Location {
    pub file: PathBuf,
    pub byte_range: Range<usize>,
    pub line: usize,
}

/// Cursor flows through operator pipe.
/// Accumulates captures from each stage.
#[derive(Debug, Clone, Default)]
pub struct Cursor {
    pub repo: String,
    pub rev: String,
    pub file: Option<PathBuf>,
    pub captures: HashMap<String, CaptureValue>,
}

/// Capture with provenance.
#[derive(Debug, Clone)]
pub struct CaptureValue {
    pub value: String,
    pub source: CaptureSource,
    /// For [repo], [rev] scan annotations
    pub scan: Option<ScanAnnotation>,
}

#[derive(Debug, Clone)]
pub enum CaptureSource {
    /// Extracted from current file by pattern
    Extracted,
    /// Grounded from cross-ref query
    Grounded { rule: String, var: String },
}

/// Scan annotations drive discovery.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ScanAnnotation {
    Repo,
    Rev,
    RepoNorm,
    RevNorm,
}

/// Context from git/config.
#[derive(Debug, Clone)]
pub struct Context {
    pub workspace: PathBuf,
    pub repo: Option<String>,
    pub rev: Option<String>,
}

/// Diagnostic with EXACT byte ranges.
/// Operators own precise location calculation.
#[derive(Debug, Clone)]
pub struct Diagnostic {
    pub message: String,
    /// Location in sprf file (the pattern/operator)
    pub rule_location: Location,
    /// Optional: location in target file being analyzed
    pub target_location: Option<TargetLocation>,
    pub severity: Severity,
    pub fix: Option<Fix>,
}

#[derive(Debug, Clone)]
pub struct TargetLocation {
    pub file: PathBuf,
    pub byte_range: Range<usize>,
    pub line: usize,
}

#[derive(Debug, Clone)]
pub struct Fix {
    pub description: String,
    pub replacement: String,
    pub range: Location,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Severity {
    Error,
    Warning,
    Info,
}

/// Cross-reference specification.
/// ${rule.$VAR} in pattern becomes this.
#[derive(Debug, Clone)]
pub struct CrossRef {
    pub rule: String,
    pub var: String,
    pub span: Location,
}

/// Operator output: cursors downstream + diagnostics + effects.
#[derive(Debug)]
pub struct OpOutput<E> {
    pub cursors: Vec<Cursor>,
    pub diagnostics: Vec<Diagnostic>,
    pub effects: Vec<E>,
}

impl<E> Default for OpOutput<E> {
    fn default() -> Self {
        Self {
            cursors: vec![],
            diagnostics: vec![],
            effects: vec![],
        }
    }
}

/// Generic effect for framework routing.
/// Operators define their own Effect types, convert to this.
#[derive(Debug)]
pub enum GenericEffect {
    Shell(ShellEffect),
    Write(FsWriteEffect),
    Refactor(RefactorEffect),
}

/// Shell execution effect (Phase 2, gated).
#[derive(Debug)]
pub struct ShellEffect {
    pub command: String,
    pub inputs: HashMap<String, String>,
    pub timeout_ms: u64,
    pub mutating: bool,
}

/// File write effect (Phase 4, approved).
#[derive(Debug)]
pub struct FsWriteEffect {
    pub path: PathBuf,
    pub content: String,
    pub backup_first: bool,
}

/// Code transform effect (Phase 4, approved).
#[derive(Debug)]
pub struct RefactorEffect {
    pub description: String,
    pub targets: Vec<TargetRange>,
    pub template: String,
}

#[derive(Debug)]
pub struct TargetRange {
    pub file: PathBuf,
    pub byte_range: Range<usize>,
}
