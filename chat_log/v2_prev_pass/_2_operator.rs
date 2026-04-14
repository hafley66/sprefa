//! Operator trait.

use crate::{Context, Cursor, OpResult};
use crate::_1_store::Store;

pub trait Operator: std::fmt::Debug + Send + Sync {
    type Effect;
    
    /// Parse from body (the content inside the parens).
    fn parse(body: &str) -> Result<Self, ParseError> where Self: Sized;
    
    /// Execute the operator.
    fn execute(&self, store: &dyn Store, input: Vec<Cursor>, ctx: &Context) 
        -> OpResult<Self::Effect>;
}

#[derive(Debug)]
pub struct ParseError {
    pub message: String,
    pub offset: usize,
}
