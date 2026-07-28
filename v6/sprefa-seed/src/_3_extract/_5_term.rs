//! The TERM-EXTRACT axis — pattern -> rows (the `sg`/`ast`/`json`/`regex`/`yaml`
//! body-item ops). A DIFFERENT axis from the four graph families: those are
//! fixed per-language analyses projecting onto nodes+edges; these are
//! user-authored patterns exploding a bound string into arbitrary rows whose
//! schema the calling rel declares. Both are "content in, facts out," both are
//! pure + content-addressed, both run on the same arena + rayon dispatch — so
//! they share infrastructure without sharing a trait. (Forcing one trait would
//! erase that graph families are family-driven and term ops are pattern-driven.)
//!
//! The unification that DOES hold: a graph-family extractor can be IMPLEMENTED
//! as a Composed bundle of ast-grep term ops (ast-grep pattern -> captures ->
//! project onto node/edge). That is the React host-vs-composite shape the seed
//! already models: `Native` is a Rust impl (a primitive the engine calls);
//! `Composed` is a .dl template expanded into existing ops. ast-grep is the
//! common primitive under both the `sg` term op and any ast-grep-backed family
//! extractor (v5 used syn/oxc/tree-sitter for families; ast-grep was term-op
//! only — v6 may lift families onto ast-grep where it pays).

use crate::_3_extract::_0_shape::NameId;

/// A produced set of rows, typed by the rel schema at the call site. Carries
/// arena-interned values (no String per cell — the v5 `Value::Str(String)`
/// disease that swelled the dictionary).
pub struct RowSet { pub rows: Vec<Row> }

#[derive(Clone, Debug)]
pub struct Row { pub cells: Vec<TermValue> }

/// A cell value. `Str` is arena-interned (`NameId`), not an owned String.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum TermValue { Int(i64), Name(NameId), Bool(bool) }

/// The pattern argument (a jsonpath / regex / ast-grep pattern / yaml path).
/// Arena-interned; the op resolves it once.
pub struct ExtractArgs { pub pattern: NameId }

/// ONE seam for every term-extract op. PUBLIC and OPEN (React component
/// contract): we ship builtins (`json`/`regex`/`ast`/`sg`/`yaml`), users
/// `impl Extractor` in their own crate and register it. No builtin privilege —
/// every op is referenced by name in `.dl` and resolved through the registry.
/// A sonnet-level task is exactly `fn extract(&[u8], &ExtractArgs) -> RowSet`:
/// bounded, pure, cannot break the fixpoint (touches only its own leaf).
pub trait Extractor: Sync {
    fn name(&self) -> &'static str;
    fn extract(&self, input: &[u8], args: &ExtractArgs, arena: &ParseStrings) -> RowSet;
}

/// The per-extraction arena string table `NameId` indexes into. Shared by the
/// parse tier, the family extractors, and the term ops so every value interns
/// once and crosses the thread boundary as a dense id.
pub struct ParseStrings;

/// Native Rust impl (host component) vs Composed .dl template (composite).
pub enum ExtractorDef {
    Native(Box<dyn Extractor>),
    Composed(ComposedOp),
}

/// A parameterized .dl op: params + a body template, expanded at lower time.
/// The "write a reusable op without touching Rust" path.
pub struct ComposedOp { pub params: Vec<NameId> }

/// name -> op. Builtins pre-registered; users add their own. Closed KERNEL
/// (the 4 rule kinds) + open UNIVERSE (these ops) = React's fixed protocol +
/// open components. New op = registry.insert.
pub struct Registry { pub ops: Vec<(NameId, ExtractorDef)> }
