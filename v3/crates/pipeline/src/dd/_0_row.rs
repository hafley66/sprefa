//! Row + Time aliases used by the DD bridge.
//!
//! Two notes pinned by chat_log/20260501.0:
//! - Capture-named encoding (Vec<(Arc<str>, Value)>) is the proposed shape
//!   for the IR-driven path in card E. For card A's synthetic smoke, the
//!   smoke uses concrete primitive tuples to match larger.rs.
//! - DD requires Row: Data + ExchangeData + Hash + Ord + Clone. The
//!   pipeline crate's existing `pipeline::value::Value` does not yet
//!   satisfy Hash + Ord uniformly across variants. Bridging that lands in
//!   card E; this card uses a local Value alias gated on what DD needs.

use std::sync::Arc;

/// Logical timestamp inside a DD scope. One sprefa generation == one
/// timely Time at W=1.
pub type Time = u64;

/// Differential weight. +1 = insert, -1 = retract; abs > 1 happens during
/// consolidation but doesn't surface at the input seam.
pub type Diff = isize;

/// Identifier for a fact (== the relation name in `fact(:name, ...)`).
pub type FactName = Arc<str>;

/// Placeholder Value for the DD bridge until card E concretizes the
/// adapter from `pipeline::value::Value`. Card A's smoke uses primitive
/// row encodings directly (Strings + u64) and never instantiates this
/// type, but it's defined here so call sites in cards E+ can refer to it.
#[derive(Clone, Debug, Eq, PartialEq, Ord, PartialOrd, Hash)]
pub enum Value {
    Str(Arc<str>),
    U64(u64),
    I64(i64),
    Bytes(Arc<[u8]>),
}

/// Capture-named row. Defined here so the surface is documented; the
/// synthetic smoke in card A does NOT use this shape. Card E wires the
/// real binding from `pipeline::value::Value` and ScriptIR fact decls.
pub type Row = Vec<(Arc<str>, Value)>;
