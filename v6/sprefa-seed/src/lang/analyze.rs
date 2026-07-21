//! THE REACTIVITY. We do NOT hand-mount clocks/@async. We ANALYZE.
//!
//! Signals just describe reads; the hard part is listening to the reads and
//! learning to invalidate. Here the "reads" are the rule -> rel reference edges
//! (DerivedRule.body atoms). That graph IS the reactivity.
//!
//! One pass over the reference graph does BOTH jobs the note called out as "the
//! same situation":
//!   1. STRATIFY  — SCC + topo order (recursion inside a stratum; negation/effects
//!                  cross strata). This is classic datalog stratification.
//!   2. PAINT     — propagate an EFFECT LATTICE up the same graph (taint/paint):
//!                  anything transitively reaching an effect/clock SOURCE is Async/
//!                  Effectful; everything else is Pure. This replaces manual @async.
//! It is the alpha/beta reference crawl: follow reads, join the lattice at each node.
//!
//! Prior art to study (not bolt-on): Koka effect rows / algebraic effects; taint /
//! information-flow; Solid signals (auto read-tracking) + Salsa/Adapton (auto
//! invalidation); datalog stratification. The escape hatch stays: `escape` lets an
//! author override the inferred class when analysis is too conservative.

use crate::key::RelId;

/// The reference graph: (reader_rel, read_rel) edges, harvested from rule bodies.
/// = the signal read-set. Invalidation propagates along these edges backwards.
pub struct RefGraph {
    pub reads: Vec<(RelId, RelId)>,
    pub effect_sources: Vec<RelId>,   // rels headed by an Effect/clock rule = taint roots
}

/// The lattice painted onto each rel (join = least-upper-bound up the graph).
/// Pure < Async < Effectful < Mutating.
#[derive(Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Debug)]
pub enum EffectClass {
    Pure,       // a function of the DB only; view-able, freely re-run
    Async,      // reaches a clock / awaitable; has pause points
    Effectful,  // reaches an impure INGEST effect (http/cmd); quarantined from the fixpoint
    Mutating,   // reaches a MUTATE effect (--move/codemod) that WRITES the World. the top:
                // must apply in a controlled commit, be idempotent, and close the reconcile
                // loop through the filesystem. NEVER inside the fixpoint.
}

/// A stratum: an SCC of the reference graph, in topological order. Recursion lives
/// INSIDE one stratum; negation and effects must cross a stratum boundary.
pub struct Stratum {
    pub rels: Vec<RelId>,
    pub order: u32,
}

/// The DERIVED verdict per rel — this is what "we must see and analyze" produces,
/// replacing hand-mounted clocks/@async. `eval` falls out of (effect x demand).
pub struct RelAnalysis {
    pub rel: RelId,
    pub effect: EffectClass,          // painted from sources up the graph
    pub stratum: u32,                 // topo stratum (negation/effect-safe order)
    pub eval: EvalStrategy,           // push/view/demand/clock — inferred, not annotated
    pub escape: Option<EffectClass>,  // manual override = the escape hatch
}

/// How a rel is run — DERIVED, not authored. "Not always running": a Pure rel with
/// no hot reader is a View (pull); a hot one is Materialized (push); an Async one
/// is Clock/Demand. Mirrors json-rx shareReplay(refCount) semantics.
pub enum EvalStrategy {
    Materialized,          // table, push-maintained on every input delta
    View,                  // SQL VIEW, zero storage, pull on read
    Demand,                // materialize on first read, maintain, evict (shareReplay refCount)
    Clock(ClockSpec),      // push on a timer (ghcacher @async clock(5,_))
}

pub struct ClockSpec { pub period_ms: u64 }

/// The whole analysis: build the ref graph, SCC/topo stratify, paint the effect
/// lattice from the sources up, then derive an eval strategy per rel. ONE pass,
/// both jobs. Bodies are the next zoom level; this pins the signature + the graph.
pub fn analyze(_graph: &RefGraph) -> Vec<RelAnalysis> {
    // 1. scc(reads) -> condensation -> topo order          => Stratum{order}
    // 2. paint: for each rel, effect = lub over (own kind, effect of everything it reads)
    //    seeded at effect_sources; fixpoint up the DAG of SCCs.
    // 3. eval = f(effect, demand): Pure+cold=View, Pure+hot=Materialized,
    //    Async=Clock/Demand, Effectful=Demand+quarantine.
    // 4. apply `escape` overrides.
    todo!("zoom-2: implement the one ref-graph pass")
}
