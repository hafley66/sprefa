//! The calculus — DENOTATIONAL only (what a relation/rule MEANS). No algorithm;
//! the runtime computes these, feldera just states the contracts. Slim on purpose.

/// Multiplicity in a Z-set: how many derivations support a fact. Add on union,
/// sub on retraction, live iff > ZERO. i64 now; a source-tracking semiring later.
pub trait Weight: Copy + Ord {
    const ZERO: Self;
    fn add(self, other: Self) -> Self;
    fn sub(self, other: Self) -> Self;
    fn is_live(self) -> bool; // self > ZERO
}

impl Weight for i64 {
    const ZERO: i64 = 0;
    fn add(self, other: i64) -> i64 { self + other }
    fn sub(self, other: i64) -> i64 { self - other }
    fn is_live(self) -> bool { self > 0 }
}

/// A relation = a map key -> weight (a Z-set). Shape only; no storage choice.
pub trait Relation {
    type Key: Ord + Copy;
    type Weight: Weight;
}

/// The unit of ALL incremental work: signed weight changes (+w add, -w retract).
pub struct Delta<K, W> {
    pub changes: Vec<(K, W)>,
}

/// Retraction cascade CONTRACT — the laws MEASURED in the sqlite lab: apply a
/// delta, settle to fixpoint; work O(delta), memory bounded, a fact dies only when
/// its LAST support hits ZERO. Implemented by the runtime over a Store, not here.
pub trait Retract: Relation {
    // fn apply(&mut self, delta: &Delta<Self::Key, Self::Weight>);  // contract only
}

/// Denotational marker: this rel IS the least fixpoint of its rules. The runtime
/// computes it semi-naively; feldera only says what it means.
pub trait Fixpoint: Relation {}
