//! Fs operator - driver, lists files.
//!
//! DESIGN: Driver operator (no upstream input, creates seed cursors).
//! Example: fs("**/Cargo.toml")
//! Ignores input cursors, produces one cursor per matching file.

use crate::{Context, Cursor, Diagnostic, Location, OpOutput, Operator, Store};

/// File system driver operator.
#[derive(Debug)]
pub struct FsOp {
    pattern: String,
    _span: Location,
}

impl Operator for FsOp {
    type Effect = ();
    
    fn parse(body: &str, span: Location) -> Result<Self, Vec<Diagnostic>> {
        let pattern = body.trim().trim_matches('"').trim_matches('\'').to_string();
        if pattern.is_empty() {
            return Err(vec![Diagnostic {
                message: "fs() requires a glob pattern".to_string(),
                rule_location: span,
                target_location: None,
                severity: crate::Severity::Error,
                fix: None,
            }]);
        }
        Ok(FsOp { pattern, _span: span })
    }
    
    fn execute(&self, store: &dyn Store, _input: Vec<Cursor>, ctx: &Context) -> OpOutput<()> {
        let files = store.list_files(&ctx.workspace, &self.pattern);
        
        if files.is_empty() {
            return OpOutput {
                cursors: vec![],
                diagnostics: vec![Diagnostic {
                    message: format!("No files matched pattern: {}", self.pattern),
                    rule_location: self._span.clone(),
                    target_location: None,
                    severity: crate::Severity::Warning,
                    fix: None,
                }],
                effects: vec![],
            };
        }
        
        let cursors: Vec<Cursor> = files.into_iter()
            .map(|f| Cursor {
                repo: ctx.repo.clone().unwrap_or_default(),
                rev: ctx.rev.clone().unwrap_or_default(),
                file: Some(f),
                captures: std::collections::HashMap::new(),
            })
            .collect();
        
        OpOutput {
            cursors,
            diagnostics: vec![],
            effects: vec![],
        }
    }
}
