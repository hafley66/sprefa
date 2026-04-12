//! Operator trait system for sprf.
//!
//! Each operator (fs, file, json, ast, etc.) implements the Operator trait,
//! handling its own parsing, validation, and execution.

use std::path::Path;
use crate::analyze::{SourceRange, Severity};

/// Context for operator validation.
pub struct ValidationContext<'a> {
    pub workspace_root: &'a Path,
    pub documents: &'a std::collections::HashMap<String, String>,
}

/// Context for operator execution.
pub struct ExecutionContext<'a> {
    pub workspace_root: &'a Path,
    pub documents: &'a std::collections::HashMap<String, String>,
}

/// Result of operator execution.
pub struct OperatorResult {
    /// Extracted values
    pub values: Vec<ExtractedValue>,
    /// Diagnostics from execution
    pub diagnostics: Vec<OperatorDiagnostic>,
}

/// A single extracted value.
pub struct ExtractedValue {
    pub value: String,
    pub file_path: String,
    pub line: Option<usize>,
}

/// Common error types across ALL operators.
#[derive(Debug, Clone)]
pub enum OperatorError {
    /// Pattern syntax error (invalid glob, malformed JSON path)
    InvalidPattern { pattern: String, reason: String },
    /// File system error (not found, permission denied)
    FileSystem { path: std::path::PathBuf, reason: String },
    /// Data format error (invalid JSON, TOML parse fail)
    DataFormat { path: std::path::PathBuf, format: String, error: String },
    /// Structural mismatch (JSON path not found in file)
    PathNotFound { attempted: String, available: Vec<String> },
}

/// Operator diagnostic (runtime error from execution).
#[derive(Debug, Clone)]
pub struct OperatorDiagnostic {
    pub error: OperatorError,
    pub location: SourceRange,
    pub rule_name: String,
    pub severity: Severity,
}

/// Every operator (fs, file, json, ast, etc.) implements this.
pub trait Operator: std::fmt::Debug + Send + Sync {
    /// Operator name for diagnostics ("fs", "json", "ast")
    fn name(&self) -> &'static str;
    
    /// Source span of this operator instance.
    fn span(&self) -> SourceRange;
    
    /// Validate at analysis time (files exist, patterns valid).
    /// Returns runtime diagnostics (file not found, path mismatch).
    fn validate(&self, ctx: &ValidationContext) -> Vec<OperatorDiagnostic>;
    
    /// Execute the operator (find files, extract values).
    fn execute(&self, ctx: &ExecutionContext) -> OperatorResult;
    
    /// Get the pattern as a string for display.
    fn pattern(&self) -> String;
}

// Re-export operator implementations
mod fs;
mod json;

pub use fs::FsOperator;
pub use json::JsonOperator;

/// Create an operator from an AST slot.
pub fn operator_from_slot(
    slot: &crate::_0_ast::Slot,
    span: SourceRange,
    rule_name: &str,
) -> Option<Box<dyn Operator>> {
    // Try each operator type
    if let Some(op) = FsOperator::from_slot(slot, span.clone(), rule_name) {
        return Some(Box::new(op));
    }
    
    if let Some(op) = JsonOperator::from_slot(slot, span.clone(), rule_name) {
        return Some(Box::new(op));
    }
    
    None
}
