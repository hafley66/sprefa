use crate::{Context, Cursor, OpResult, Operator, Store};
use crate::_2_operator::ParseError;
use crate::_4_parse_utils::glob_match;

#[derive(Debug)]
pub struct RevOp {
    pattern: String,
}

impl Operator for RevOp {
    type Effect = ();
    
    fn parse(body: &str) -> Result<Self, ParseError> {
        let pattern = body.trim().trim_matches('"').trim_matches('\'');
        if pattern.is_empty() {
            return Err(ParseError { message: "rev() requires pattern".into(), offset: 0 });
        }
        Ok(RevOp { pattern: pattern.to_string() })
    }
    
    fn execute(&self, _store: &dyn Store, input: Vec<Cursor>, ctx: &Context) -> OpResult<()> {
        let mut out = OpResult::default();
        
        if input.is_empty() {
            if glob_match(&self.pattern, &ctx.rev) {
                out.cursors.push(Cursor {
                    repo: ctx.repo.clone(),
                    rev: ctx.rev.clone(),
                    ..Default::default()
                });
            }
        } else {
            for cursor in input {
                if glob_match(&self.pattern, &cursor.rev) {
                    out.cursors.push(cursor);
                }
            }
        }
        
        out
    }
}
