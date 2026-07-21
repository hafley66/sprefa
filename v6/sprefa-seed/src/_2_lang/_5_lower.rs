//! AST + analysis -> the physical PLAN. Two jobs:
//!   - NORMALIZE dot access: every Term::Proj lowers to a join (or record lookup /
//!     ADT match / fan-out) per its FieldKind. Datalog = the normalize+flatten functor.
//!   - Emit a per-rel plan node carrying its derived EvalStrategy + stratum, so the
//!     runtime knows push vs pull vs clock without any authored annotation.

use crate::_0_key::RelId;
use crate::_2_lang::_4_analyze::EvalStrategy;

pub struct Plan {
    pub nodes: Vec<PlanNode>,
}

pub struct PlanNode {
    pub rel: RelId,
    pub eval: EvalStrategy,   // from analyze
    pub stratum: u32,
    // relational algebra (joins from normalized dot access, semi-naive recursion) -> zoom-2
}
