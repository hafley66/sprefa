//! Rev operator - filter by rev pattern.
//!
//! DESIGN: Filters by branch/tag pattern. Regex or glob.
//! Example: rev("main") or rev("release-*")

use crate::{Context, Cursor, Diagnostic, Location, OpOutput, Operator, Store};
use crate::parse_utils::glob_match;

/// Rev filter operator.
#[derive(Debug)]
pub struct RevOp {
    pattern: String,
    _span: Location,
}

impl Operator for RevOp {
    type Effect = ();
    
    fn parse(body: &str, span: Location) -> Result<Self, Vec<Diagnostic>> {
        let pattern = body.trim().trim_matches('"').trim_matches('\'').to_string();
        if pattern.is_empty() {
            return Err(vec![Diagnostic {
                message: "rev() requires a pattern".to_string(),
                rule_location: span,
                target_location: None,
                severity: crate::Severity::Error,
                fix: None,
            }]);
        }
        Ok(RevOp { pattern, _span: span })
    }
    
    fn execute(&self, _store: &dyn Store, input: Vec<Cursor>, _ctx: &Context) -> OpOutput<()> {
        let cursors: Vec<Cursor> = input.into_iter()
            .filter(|c| glob_match(&self.pattern, &c.rev))
            .collect();
        
        OpOutput {
            cursors,
            diagnostics: vec![],
            effects: vec![],
        }
    }
}
