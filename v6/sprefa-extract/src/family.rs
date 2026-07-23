//! S2: families are TYPE-LEVEL. `Family` trait + marker structs; `Node<F>` /
//! `Edge<F>` are generic over the marker. The `NodeKind`/`EdgeKind` sums of the
//! seed (`_0_shape.rs`) are deleted: orthogonal axes are not variants of one
//! type, and the store splits by family anyway.
//!
//! Three planes, five families:
//!   RESOLUTION (SCIP-wire, ratchet-able): CallF, TypeF, ModuleF
//!   VALUE-FLOW (native, AST-only):       DfF (+ typed Flow edges)
//!   STRUCTURE (lossless CST):            CstF
//!
//! Commit 1 exercises ONLY `CstF` (the structure plane; the piping-proof
//! family). The other markers land with their commits (2-6).

use std::fmt;

use crate::shape::{FamilyTag, NameId};

/// One static-analysis family. The associated kinds are the per-family node and
/// edge vocabularies; `TAG` is the flat discriminant used at the seam only.
pub trait Family {
    type NodeKind: Clone + fmt::Debug;
    type EdgeKind: Copy + Clone + fmt::Debug;
    const TAG: FamilyTag;
}

// ── STRUCTURE plane (commit 1) ──────────────────────────────────────────────

/// The lossless named-node tree (the tree-sitter CST, reached through ast-grep's
/// grammars). `NodeKind` is an OPEN grammar vocabulary interned as a `NameId`
/// (`function_declaration`, `interface_declaration`, ...); it is not a closed
/// enum. The single edge kind is `Child`.
#[derive(Default, Copy, Clone, Debug)]
pub struct CstF;

#[derive(Copy, Clone, Debug, PartialEq, Eq)]
pub enum CstEdgeKind {
    /// src = parent named node, dst = child named node. Unnamed punctuation
    /// nodes are not emitted; their named descendants reparent to the nearest
    /// named ancestor (port of v5 `src/cst.rs::walk_cst`).
    Child,
}

impl Family for CstF {
    type NodeKind = NameId;
    type EdgeKind = CstEdgeKind;
    const TAG: FamilyTag = FamilyTag::Cst;
}
