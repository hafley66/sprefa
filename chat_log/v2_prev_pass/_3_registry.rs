//! Operator registry.

use std::collections::HashMap;
use crate::{Cursor, Diagnostic};

pub struct Registry {
    parsers: HashMap<String, Box<dyn Fn(&str) -> Result<Box<dyn DynOperator>, String> + Send + Sync>>,
}

impl Registry {
    pub fn new() -> Self {
        let mut r = Self { parsers: HashMap::new() };
        
        use crate::operators::*;
        r.register::<RepoOp>("repo");
        r.register::<RevOp>("rev");
        r.register::<FsOp>("fs");
        r.register::<JsonOp>("json");
        
        r
    }
    
    fn register<T: crate::Operator + 'static>(&mut self, name: &str) {
        self.parsers.insert(name.to_string(), Box::new(|body| {
            T::parse(body)
                .map(|op| Box::new(op) as Box<dyn DynOperator>)
                .map_err(|e| e.message)
        }));
    }
    
    pub fn parse(&self, name: &str, body: &str) -> Result<Box<dyn DynOperator>, String> {
        self.parsers.get(name)
            .ok_or_else(|| format!("Unknown operator: {}", name))?
            (body)
    }
}

/// Object-safe operator trait.
pub trait DynOperator: std::fmt::Debug + Send + Sync {
    fn execute_dyn(&self, store: &dyn crate::Store, input: Vec<Cursor>, ctx: &crate::Context)
        -> (Vec<Cursor>, Vec<Diagnostic>, Vec<Box<dyn std::any::Any>>);
}

impl<T: crate::Operator + 'static> DynOperator for T {
    fn execute_dyn(&self, store: &dyn crate::Store, input: Vec<Cursor>, ctx: &crate::Context)
        -> (Vec<Cursor>, Vec<Diagnostic>, Vec<Box<dyn std::any::Any>>) 
    {
        let out = self.execute(store, input, ctx);
        let effects = out.effects.into_iter()
            .map(|e| Box::new(e) as Box<dyn std::any::Any>)
            .collect();
        (out.cursors, out.diagnostics, effects)
    }
}
