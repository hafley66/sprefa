//! Operator trait.
//!
//! DESIGN: Each operator is self-contained.
//! - Owns its parsing (parse body string)
//! - Owns its error types
//! - Owns exact byte range calculation for diagnostics
//! - Pure operators have Effect = ()
//! - Mutating operators return specific Effect type

use crate::{Context, Cursor, Diagnostic, Location, OpOutput, Store};

/// Operator trait.
/// 
/// E = Effect type. () for pure ops, specific enum for mutating ops.
pub trait Operator: std::fmt::Debug + Send + Sync {
    type Effect;
    
    /// Parse from sprf body.
    /// Span is location in sprf file for diagnostic attribution.
    fn parse(body: &str, span: Location) -> Result<Self, Vec<Diagnostic>>
    where Self: Sized;
    
    /// Execute with Store for reads.
    /// Input cursors come from upstream operator.
    /// Returns: downstream cursors + diagnostics + effects.
    fn execute(&self, store: &dyn Store, input: Vec<Cursor>, ctx: &Context) 
        -> OpOutput<Self::Effect>;
}

/// Object-safe operator wrapper for registry.
pub trait DynOperator: std::fmt::Debug + Send + Sync {
    fn execute_dyn(&self, store: &dyn Store, input: Vec<Cursor>, ctx: &Context) 
        -> (Vec<Cursor>, Vec<Diagnostic>, Vec<Box<dyn std::any::Any>>);
}

impl<T: Operator + 'static> DynOperator for T {
    fn execute_dyn(&self, store: &dyn Store, input: Vec<Cursor>, ctx: &Context) 
        -> (Vec<Cursor>, Vec<Diagnostic>, Vec<Box<dyn std::any::Any>>) 
    {
        let out = self.execute(store, input, ctx);
        let effects: Vec<Box<dyn std::any::Any>> = out.effects.into_iter()
            .map(|e| Box::new(e) as Box<dyn std::any::Any>)
            .collect();
        (out.cursors, out.diagnostics, effects)
    }
}
