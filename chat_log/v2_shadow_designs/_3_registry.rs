//! Operator registry.
//!
//! Maps operator names to parse functions.
//! Framework knows "json" -> JsonOperator::parse.

use std::collections::HashMap;
use crate::{Diagnostic, DynOperator, Location};

/// Registry of operator constructors.
pub struct Registry {
    parsers: HashMap<String, Box<dyn Fn(&str, Location) -> Result<Box<dyn DynOperator>, Vec<Diagnostic>> + Send + Sync>>,
}

impl Registry {
    pub fn new() -> Self {
        use crate::operators::*;
        
        let mut r = Self { parsers: HashMap::new() };
        
        // Register foundation operators
        r.register::<RepoOp>("repo");
        r.register::<RevOp>("rev");
        r.register::<FsOp>("fs");
        r.register::<JsonOp>("json");
        
        r
    }
    
    fn register<T: crate::Operator + 'static>(&mut self, name: &str) {
        self.parsers.insert(name.to_string(), Box::new(|body, span| {
            T::parse(body, span)
                .map(|op| Box::new(op) as Box<dyn DynOperator>)
        }));
    }
    
    pub fn parse(&self, name: &str, body: &str, span: Location) 
        -> Result<Box<dyn DynOperator>, Vec<Diagnostic>> 
    {
        match self.parsers.get(name) {
            Some(parser) => parser(body, span),
            None => Err(vec![Diagnostic {
                message: format!("Unknown operator: {}", name),
                rule_location: span,
                target_location: None,
                severity: crate::Severity::Error,
                fix: None,
            }]),
        }
    }
}

impl Default for Registry {
    fn default() -> Self { Self::new() }
}
