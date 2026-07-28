//! Per-family fact structs — v5's `TypeFacts`/`CallFacts`/`DataflowFacts` +
//! `ModuleRef`, re-expressed for the normalized shape.
//!
//! What changed from v5 (every change deletes a piece of the coordinate-string
//! disease):
//!  - `sym: String` / `fn_sym: String` -> GONE. Identity is the node's `Span`;
//!    an enclosing callable is its own node, referenced by `Span` (resolved to a
//!    `node_id` at the seam by joining `(blob, span, kind)`). `mint_sym` deleted.
//!  - `file: String` / `line: u32` / `col: u32` -> `Span { start, len }`.
//!  - `kind: String` (df) / `&'static str` (edges) -> the typed enums in `_0_shape`.
//!  - cross-family references (df node -> its enclosing fn) -> a `Span` (the
//!    enclosing `CallDef`'s coordinate), not a qualified sym string.
//! What is UNCHANGED: the field sets, the aux projections (param_pos/args/fields/
//! lits/loops/nests/docs/consts/sigs), the binding side table. v5 got these
//! right; they are lifted with type swaps only.

use crate::_3_extract::_0_shape::{
    BlobHash, CallEdgeKind, CallKind, ConstValueKind, DfNodeKind, ModuleRefKind, NameId, NodeRef,
    ProjectEdge, RawEdge, Resolution, Span, TypeEntityKind,
};
use crate::_3_extract::_1_mask::Binding;

// ════════════════════════════════════════════════════════════════════════════
// DATAFLOW (intra-procedural value flow) — AST-only, SCIP cannot produce it
// ════════════════════════════════════════════════════════════════════════════
#[derive(Clone, Debug, Default)]
pub struct DfFileFacts {
    pub nodes: Vec<DfNode>,
    pub edges: Vec<RawEdge>,        // all EdgeKind::Df
    pub loops: Vec<LoopFact>,       // loop-carried / Big-O intentions
    pub allocators: Vec<Span>,      // enclosing-callable spans whose body builds a collection
    pub nests: Vec<NestFact>,       // call nested in loop (Big-O composition)
    pub param_pos: Vec<(NodeRef, u32)>,           // param node -> positional index (aligns type_sig.pos)
    pub args: Vec<DfArg>,           // call/new node -> positional arg node (receiver = -1)
    pub fields: Vec<DfField>,       // struct/object-literal node -> named field -> value
    pub lits: Vec<DfLit>,           // string-carrying value nodes
}

/// A df node. `scope` is the enclosing callable's span (v5 `fn_sym`), used to
/// join back to the call family without a sym string. `var` is the variable
/// name when the kind is var-related (var_read/var_write/let_bind/param/borrow).
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct DfNode {
    pub span: Span,
    pub kind: DfNodeKind,
    pub var: Option<NameId>,
    pub scope: Span,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct LoopFact { pub span: Span, pub var: Option<NameId>, pub collection: Option<NameId>, pub scope: Span }

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct NestFact { pub call: NodeRef, pub loop_span: Span, pub depth: u32, pub collection: Option<NameId> }

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct DfArg { pub call: NodeRef, pub pos: i64, pub arg: NodeRef }

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct DfField { pub node: NodeRef, pub field: NameId, pub value: NodeRef }

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct DfLit { pub node: NodeRef, pub text: NameId, pub kind: ConstValueKind }

// ════════════════════════════════════════════════════════════════════════════
// CALL (inter-procedural call graph)
// ════════════════════════════════════════════════════════════════════════════
/// Phase-1: defs + unresolved sites. Phase-2 (`CallProjectFacts`) resolves each
/// site's callee to a `ProjectEdge`. The caller of a site is derived by span
/// containment at the seam (v5 `resolve_caller`), so it is not stored.
#[derive(Clone, Debug, Default)]
pub struct CallFileFacts { pub defs: Vec<CallDef>, pub sites: Vec<CallSite> }

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct CallDef { pub span: Span, pub kind: CallKind, pub body_end: u32, pub name: NameId }

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct CallSite { pub span: Span, pub callee: NameId, pub callee_path: Option<NameId> }

/// Phase-2: resolved caller -> callee edges. `kind` records HOW the callee was
/// resolved (name match vs SCIP override) — v5's plain-text call_edge kind.
#[derive(Clone, Debug, Default)]
pub struct CallProjectFacts { pub edges: Vec<ProjectEdgeCall> }

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct ProjectEdgeCall { pub edge: ProjectEdge, pub kind: CallEdgeKind }

// ════════════════════════════════════════════════════════════════════════════
// TYPE (declared types + structural edges)
// ════════════════════════════════════════════════════════════════════════════
#[derive(Clone, Debug, Default)]
pub struct TypeFileFacts {
    pub entities: Vec<TypeEntity>,
    pub edges: Vec<RawEdge>,        // intra-file type edges, EdgeKind::Type
    pub docs: Vec<DocFact>,
    pub consts: Vec<ConstValueFact>,
    pub sigs: Vec<TypeSig>,         // arrow-type / signature slots
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct TypeEntity { pub span: Span, pub kind: TypeEntityKind, pub name: NameId, pub parent: Option<NameId> }

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct TypeSig { pub owner: Span, pub slot: SigSlot, pub pos: u32, pub ty: TypeRef }

#[derive(Copy, Clone, Debug, PartialEq, Eq)]
pub enum SigSlot { Param, Ret, Field }

/// A type reference. v5 `TypeRef { Named | Resolved | Unresolved }` with String;
/// here the resolved arm is a content key + span (the target type entity), and
/// the others are bare `NameId`s the project phase resolves.
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum TypeRef { Named(NameId), Resolved { blob: BlobHash, span: Span }, Unresolved(NameId) }

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct DocFact { pub owner: Span, pub text: NameId, pub tags: Vec<DocTag> }

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct DocTag { pub tag: NameId, pub arg: NameId, pub text: NameId }

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct ConstValueFact { pub span: Span, pub field: Option<NameId>, pub text: NameId, pub kind: ConstValueKind }

/// Phase-2: resolved type edges + cross-type links (SCIP-resolved sym-to-sym).
#[derive(Clone, Debug, Default)]
pub struct TypeProjectFacts { pub edges: Vec<ProjectEdge>, pub links: Vec<ProjectEdge> }

// ════════════════════════════════════════════════════════════════════════════
// MODULE (file->file import graph + binding side table)
// ════════════════════════════════════════════════════════════════════════════
/// Phase-1: raw import statements. `specifier` is unresolved here (a specifier
/// is a string that means nothing without the file set). Phase-2 resolves each.
#[derive(Clone, Debug, Default)]
pub struct ModuleFileFacts { pub refs: Vec<ModuleRef> }

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct ModuleRef { pub span: Span, pub specifier: NameId, pub kind: ModuleRefKind }

/// Phase-2: resolved file->file edges + the binding side table + the resolution
/// verdict per phase-1 ref (Local / External / Unresolved).
#[derive(Clone, Debug, Default)]
pub struct ModuleProjectFacts {
    pub edges: Vec<ProjectEdge>,
    pub bindings: Vec<Binding>,
    pub resolutions: Vec<(NodeRef, Resolution)>,  // (ref index -> outcome)
}

// (EdgeKind::Type is what the intra-file RawEdge carries; re-exported for callers
//  that build type edges.)
#[allow(unused_imports)]
use _0_shape_re_exports::*;
mod _0_shape_re_exports {
    #![allow(dead_code)]
    pub use crate::_3_extract::_0_shape::{TypeEdgeKind};
}
