//! ONE seam for ALL term-extract ops. json/regex/ast/sg/yaml are REGISTERED impls,
//! never new rule kinds, tokens, or engine branches. Adding an op = impl Extractor
//! + one registry line (proof of the "no new built-in" simplicity claim). A
//! sonnet-level task is exactly `fn extract(&str) -> RowSet` — bounded, pure,
//! cannot break the fixpoint (touches only its own leaf).

use crate::_0_key::SymId;

/// A produced set of rows (typed by the rel schema at the call site). Placeholder
/// shape in the seed.
pub struct RowSet {
    pub rows: Vec<Vec<Value>>,
}

pub enum Value { Int(i64), Sym(SymId), Str(String), Bool(bool) }

pub struct ExtractArgs {
    pub path: String,   // e.g. a jsonpath / regex / ast pattern
}

/// The seam — PUBLIC and OPEN, exactly like a React component contract. We ship
/// builtins; users `impl Extractor` in their OWN Rust crate and register it. No
/// builtin privilege: every op is referenced by name in `.dl` and resolved through
/// the same registry. `input` is the bound string the rule explodes.
pub trait Extractor {
    fn extract(&self, input: &str, args: &ExtractArgs) -> RowSet;
}

/// Ops are OPEN, and come in two flavors mirroring React host vs composite:
///   Native   — a Rust `impl Extractor` (a primitive the engine calls) = host component.
///   Composed — a `.dl` template expanded into existing ops/rules, no Rust = composite.
/// Both are used identically at the call site (`foo(blob, "$.x", out)`).
pub enum ExtractorDef {
    Native(Box<dyn Extractor>),
    Composed(ComposedOp),
}

/// A parameterized `.dl` op: params + a body template, expanded at lower time.
/// The "write a reusable op without touching Rust" path (composite component).
pub struct ComposedOp {
    pub params: Vec<SymId>,
    // body template -> expanded against the call's args during lowering.
}

/// name -> op. Builtins are pre-registered; users add their own. Closed KERNEL
/// (the 4 rule kinds) + open UNIVERSE (these ops) = React's fixed protocol + open
/// components. New op = registry.insert("yaml", ExtractorDef::Native(Box::new(Yaml))).
pub struct Registry {
    pub ops: Vec<(SymId, ExtractorDef)>,
}
