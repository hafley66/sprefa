use crate::{Context, Cursor, OpResult, Operator, Store};
use crate::_2_operator::ParseError;

#[derive(Debug)]
pub struct FsOp {
    pattern: String,
}

impl Operator for FsOp {
    type Effect = ();
    
    fn parse(body: &str) -> Result<Self, ParseError> {
        let pattern = body.trim().trim_matches('"').trim_matches('\'');
        if pattern.is_empty() {
            return Err(ParseError { message: "fs() requires pattern".into(), offset: 0 });
        }
        Ok(FsOp { pattern: pattern.to_string() })
    }
    
    fn execute(&self, store: &dyn Store, input: Vec<Cursor>, ctx: &Context) -> OpResult<()> {
        let mut out = OpResult::default();
        
        let files = store.list_files(&self.pattern);
        
        if files.is_empty() {
            out.diagnostics.push(crate::Diagnostic {
                message: format!("No files matched: {}", self.pattern),
                rule_location: crate::Location {
                    file: std::path::PathBuf::from("test.spf"),
                    byte_range: 0..self.pattern.len(),
                    line: 0,
                },
                severity: crate::Severity::Warning,
            });
            return out;
        }
        
        // If input cursors, expand each
        if input.is_empty() {
            // Seed from context
            for file in files {
                out.cursors.push(Cursor {
                    repo: ctx.repo.clone(),
                    rev: ctx.rev.clone(),
                    file: Some(file),
                    captures: std::collections::HashMap::new(),
                });
            }
        } else {
            // Cross product: each cursor x each file
            for cursor in input {
                for file in &files {
                    let mut c = cursor.clone();
                    c.file = Some(file.clone());
                    out.cursors.push(c);
                }
            }
        }
        
        out
    }
}
