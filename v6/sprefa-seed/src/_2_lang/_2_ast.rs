//! The typed AST. Note what is NOT here: a rel does NOT carry an eval strategy or
//! an @async flag. Those are DERIVED by lang::analyze from the reference graph
//! (inference, not manual mounting). The author writes rules; the analyzer paints.

use crate::_0_key::{ColId, RelId, SymId, VarId};
use crate::_2_lang::_0_types::Type;

pub struct Rel {
    pub name: RelId,
    pub schema: Vec<(ColId, Type)>,
    pub rules: Vec<Rule>,   // INVARIANT: all one Rule kind (the DELETE FROM rel bug)
}

/// The four rule kinds — the FIXED kernel. Each is the sole holder of one
/// invariant; three collapses are documented v5 data-loss/non-termination bugs.
/// Extract/Effect are parameterized by a REGISTRY name, so new ops are registered
/// impls, never new rule kinds (see extract::Extractor, runtime::Effect).
pub enum Rule {
    Source(SourceRule),   // EDB: World -> Facts. effectful read, reconciled per-file.
    Derived(DerivedRule), // IDB: Rels -> Rels. pure, recursive, retraction-cascaded.
    Extract(ExtractRule), // term-extract: a registered extractor over a bound string.
    Effect(EffectRule),   // async: a registered effect (http/cmd/clock), cached+cancelled.
}

pub struct SourceRule {
    pub extractor: SymId,     // registry name (scan/regex/ast/sg/json/...)
    pub world: WorldInput,
    pub outputs: Vec<ColId>,
}

pub struct DerivedRule {
    pub head: RelId,
    pub body: Vec<BodyItem>,  // the reference edges analyze reads
}

pub struct ExtractRule {
    pub extractor: SymId,     // registry name; NOT a keyword built-in
    pub input: Term,          // the bound string it explodes
    pub args: Args,
    pub outputs: Vec<ColId>,
}

pub struct EffectRule {
    pub effect: SymId,        // registry name (http/cmd/clock/graphql/...)
    pub inputs: Vec<Term>,    // bound facts feeding the effect (also the cache key)
    pub cache: CachePolicy,   // eviction; skip-if-same-digest is inherent
}

pub struct WorldInput { pub glob: SymId }
pub struct Args(pub Vec<Term>);

pub enum BodyItem {
    Atom(RelId, Vec<Term>),   // a read: rel(args)          -> a reference edge
    Neg(RelId, Vec<Term>),    // negation: !rel(args)       -> crosses a stratum
}

pub enum Term {
    Const(SymId),
    Col(ColId),
    Proj(Box<Term>, SymId),   // DOT ACCESS: term.field; lowering dispatched by FieldKind
    Var(VarId),               // logic variable; reserved for Prolog unification
}

/// Mirror of hafley-rxjs makeSwitchMapCached's ttl/resetOnRefCountZero. The
/// skip-if-same-input-digest is inherent (cache lookup by key); this is eviction.
pub enum CachePolicy {
    Demand,       // drop on refCount -> 0
    Ttl(u64),     // keep ttl ms after last unsubscribe
    Pin,          // never evict
}
