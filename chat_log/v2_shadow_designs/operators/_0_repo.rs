//! Repo operator - filter AND set context.
//!
//! DESIGN: repo("myorg/*") filters AND sets cursor.repo.
//! Can be used as scoped block: repo("myorg/*") { fs(...) > json(...) }

use crate::{Context, Cursor, Diagnostic, Location, OpOutput, Operator, Store};
use crate::parse_utils::glob_match;

/// Repo operator.
#[derive(Debug)]
pub struct RepoOp {
    pattern: String,
    capture: Option<String>,  // $REPO from repo($REPO)
    span: Location,
}

impl Operator for RepoOp {
    type Effect = ();
    
    fn parse(body: &str, span: Location) -> Result<Self, Vec<Diagnostic>> {
        let trimmed = body.trim();
        
        // Check for capture: repo($REPO) or repo("myorg/*")
        let (pattern, capture) = if trimmed.starts_with('$') {
            // repo($REPO) - matches any, captures value
            let cap = trimmed[1..].trim().trim_matches(')').to_string();
            ("*".to_string(), Some(cap))
        } else {
            // repo("myorg/*") or repo(myorg/*) - filter pattern
            let pat = trimmed.trim_matches('"').trim_matches('\'').to_string();
            if pat.is_empty() {
                return Err(vec![Diagnostic {
                    message: "repo() requires a pattern or capture".to_string(),
                    rule_location: span,
                    target_location: None,
                    severity: crate::Severity::Error,
                    fix: None,
                }]);
            }
            (pat, None)
        };
        
        Ok(RepoOp { pattern, capture, span })
    }
    
    fn execute(&self, store: &dyn Store, input: Vec<Cursor>, ctx: &Context) -> OpOutput<()> {
        let mut out = OpOutput::default();
        
        for cursor in input {
            if glob_match(&self.pattern, &cursor.repo) {
                // If capture var specified, extract repo name
                if let Some(ref cap) = self.capture {
                    let mut new_cursor = cursor.clone();
                    new_cursor.captures.insert(cap.clone(), crate::CaptureValue {
                        value: cursor.repo.clone(),
                        source: crate::CaptureSource::Extracted,
                        scan: Some(crate::ScanAnnotation::Repo),
                    });
                    out.cursors.push(new_cursor);
                } else {
                    out.cursors.push(cursor);
                }
            }
        }
        
        // If no input cursors, seed from context
        if input.is_empty() && out.cursors.is_empty() {
            if let Some(ref repo) = ctx.repo {
                if glob_match(&self.pattern, repo) {
                    out.cursors.push(Cursor {
                        repo: repo.clone(),
                        rev: ctx.rev.clone().unwrap_or_default(),
                        file: None,
                        captures: std::collections::HashMap::new(),
                    });
                }
            }
        }
        
        out
    }
}
