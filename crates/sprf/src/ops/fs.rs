//! File system operators: fs, file, folder.

use std::path::Path;
use globset::Glob;
use walkdir::WalkDir;

use super::{Operator, ValidationContext, ExecutionContext, OperatorResult, OperatorDiagnostic, OperatorError, ExtractedValue};
use crate::analyze::SourceRange;

/// File system operator (fs, file, folder).
#[derive(Debug, Clone)]
pub struct FsOperator {
    /// Tag type: "fs", "file", or "folder"
    pub tag: String,
    /// Pattern content (e.g., "**/Cargo.toml")
    pub pattern: String,
    /// Source span
    pub span: SourceRange,
    /// Rule name for diagnostics
    pub rule_name: String,
}

impl FsOperator {
    /// Create from AST slot.
    pub fn from_slot(
        slot: &crate::_0_ast::Slot,
        span: SourceRange,
        rule_name: &str,
    ) -> Option<Self> {
        use crate::_0_ast::Tag;
        
        if let crate::_0_ast::Slot::Tagged { tag, body, .. } = slot {
            let tag_name = match tag {
                Tag::Fs => "fs",
                Tag::File => "file",
                Tag::Folder => "folder",
                _ => return None,
            };
            
            Some(Self {
                tag: tag_name.to_string(),
                pattern: body.clone(),
                span,
                rule_name: rule_name.to_string(),
            })
        } else {
            None
        }
    }
    
    /// Find matching files.
    fn find_files(&self, workspace_root: &Path) -> Vec<std::path::PathBuf> {
        let mut matches = Vec::new();
        
        // Build glob pattern
        let glob_pattern = if self.pattern.starts_with("**/") {
            self.pattern.clone()
        } else {
            format!("**/ {}", self.pattern)
        };
        
        if let Ok(glob) = Glob::new(&glob_pattern) {
            let matcher = glob.compile_matcher();
            
            for entry in WalkDir::new(workspace_root)
                .max_depth(10)
                .into_iter()
                .filter_map(|e| e.ok())
            {
                let path = entry.path();
                if path.is_file() || (self.tag == "folder" && path.is_dir()) {
                    let relative = path.strip_prefix(workspace_root).unwrap_or(path);
                    if matcher.is_match(relative) || matcher.is_match(path) {
                        matches.push(path.to_path_buf());
                    }
                }
            }
        }
        
        // Deduplicate and limit
        matches.sort();
        matches.dedup();
        matches.truncate(50);
        matches
    }
}

impl Operator for FsOperator {
    fn name(&self) -> &'static str {
        match self.tag.as_str() {
            "fs" => "fs",
            "file" => "file",
            "folder" => "folder",
            _ => "fs",
        }
    }
    
    fn span(&self) -> SourceRange {
        self.span.clone()
    }
    
    fn validate(&self, ctx: &ValidationContext) -> Vec<OperatorDiagnostic> {
        let mut diagnostics = Vec::new();
        
        let files = self.find_files(ctx.workspace_root);
        
        if files.is_empty() {
            diagnostics.push(OperatorDiagnostic {
                error: OperatorError::InvalidPattern {
                    pattern: self.pattern.clone(),
                    reason: "No files matched this pattern".to_string(),
                },
                location: self.span.clone(),
                rule_name: self.rule_name.clone(),
                severity: crate::analyze::Severity::Error,
            });
        }
        
        diagnostics
    }
    
    fn execute(&self, ctx: &ExecutionContext) -> OperatorResult {
        let files = self.find_files(ctx.workspace_root);
        let mut values = Vec::new();
        
        for file_path in files.iter().take(50) {
            let value = file_path
                .strip_prefix(ctx.workspace_root)
                .unwrap_or(file_path)
                .to_string_lossy()
                .to_string();
            
            values.push(ExtractedValue {
                value,
                file_path: file_path.to_string_lossy().to_string(),
                line: None,
            });
        }
        
        OperatorResult {
            values,
            diagnostics: Vec::new(),
        }
    }
    
    fn pattern(&self) -> String {
        self.pattern.clone()
    }
}
