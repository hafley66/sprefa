//! ONE seam for ALL term-extract ops. json/regex/ast/sg/yaml are REGISTERED impls,
//! never new rule kinds, tokens, or engine branches. Adding an op = impl Extractor
//! + one registry line (proof of the "no new built-in" simplicity claim). A
//! sonnet-level task is exactly `fn extract(&str) -> RowSet` — bounded, pure,
//! cannot break the fixpoint (touches only its own leaf).

use crate::key::SymId;

/// A produced set of rows (typed by the rel schema at the call site). Placeholder
/// shape in the seed.
pub struct RowSet {
    pub rows: Vec<Vec<Value>>,
}

pub enum Value { Int(i64), Sym(SymId), Str(String), Bool(bool) }

pub struct ExtractArgs {
    pub path: String,   // e.g. a jsonpath / regex / ast pattern
}

/// The seam. `input` is the bound string the rule explodes.
pub trait Extractor {
    fn extract(&self, input: &str, args: &ExtractArgs) -> RowSet;
}

/// name -> handler. New op = registry.insert("yaml", Box::new(Yaml)).
pub struct Registry {
    pub handlers: Vec<(SymId, Box<dyn Extractor>)>,
}
