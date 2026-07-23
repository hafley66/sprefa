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

// ── RESOLUTION plane: TypeF (commit 2b) ─────────────────────────────────────

/// The type graph: declared entities (class / interface / alias / enum /
/// function / method) + their structural edges. Commit 2b ports v5
/// `ts_entities_from` to emit the entity NODES (span + kind + name). The type
/// EDGES (field / impl / uses / ...) are name-resolved relationships and land
/// with `Resolve<TypeF>` (commit 4, scip-typescript); phase 1 stays pure-content
/// span nodes.
#[derive(Default, Copy, Clone, Debug)]
pub struct TypeF;

/// type_entity kind. v5 `decls.rs` brand, 9 variants. Struct/Trait are Rust-only;
/// TS emits Class/Interface/Alias/Enum/Function/Method.
#[derive(Copy, Clone, Debug, PartialEq, Eq)]
pub enum TypeEntityKind {
    Struct,
    Enum,
    Trait,
    Class,
    Interface,
    Alias,
    Function,
    Method,
    Const,
}

impl TypeEntityKind {
    pub fn as_str(self) -> &'static str {
        match self {
            TypeEntityKind::Struct => "struct",
            TypeEntityKind::Enum => "enum",
            TypeEntityKind::Trait => "trait",
            TypeEntityKind::Class => "class",
            TypeEntityKind::Interface => "interface",
            TypeEntityKind::Alias => "alias",
            TypeEntityKind::Function => "function",
            TypeEntityKind::Method => "method",
            TypeEntityKind::Const => "const",
        }
    }
}

/// type_edge kind. v5 `decls.rs` brand, 7 variants. Emitted by `Resolve<TypeF>`
/// (commit 4); declared here so the vocabulary is closed up front.
#[derive(Copy, Clone, Debug, PartialEq, Eq)]
pub enum TypeEdgeKind {
    Field,
    Variant,
    Impl,
    Generic,
    Param,
    Returns,
    Uses,
}

impl TypeEdgeKind {
    pub fn as_str(self) -> &'static str {
        match self {
            TypeEdgeKind::Field => "field",
            TypeEdgeKind::Variant => "variant",
            TypeEdgeKind::Impl => "impl",
            TypeEdgeKind::Generic => "generic",
            TypeEdgeKind::Param => "param",
            TypeEdgeKind::Returns => "returns",
            TypeEdgeKind::Uses => "uses",
        }
    }
}

impl Family for TypeF {
    type NodeKind = TypeEntityKind;
    type EdgeKind = TypeEdgeKind;
    const TAG: FamilyTag = FamilyTag::Type;
}
