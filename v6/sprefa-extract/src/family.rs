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

use crate::shape::{FamilyTag, NameId, Span};

/// One static-analysis family. The associated kinds are the per-family node and
/// edge vocabularies; `TAG` is the flat discriminant used at the seam only.
/// `Aux` is the family's side-channel payload (TypeF arrow-type sigs, later
/// df param_pos/args/...): per-node/per-occurrence attributes that are NOT
/// span-pair edges and do not fit the uniform `Node<F>`/`Edge<F>` shape. The
/// bundle carries one `F::Aux`; the wire flattens it to its own `FlatFact` arm.
pub trait Family {
    type NodeKind: Clone + fmt::Debug;
    type EdgeKind: Copy + Clone + fmt::Debug;
    type Aux: Default + Clone + fmt::Debug;
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
    type Aux = ();
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

// ── TypeF aux: the arrow-type payload (the D-arrow-type decision in code) ────
//
// A function/method IS a type: `[...A] => B`. The callable ENTITY node (above)
// is span + kind + name; the arrow SIGNATURE is this side table. One `TypeSig`
// per named type reference in a param slot or the return slot. The target is a
// bare `NameId` (phase-1 honest: unresolved; `Resolve<TypeF>` binds it to a
// declaration span at commit 4). v5 modeled these as name-string `param`/
// `returns` type-edges; the name survives, the sym-string does not.
//
// `pos` preserves parameter ORDER for the node-level type join (`param_pos`)
// the seed ports later. Keyword types (number/string/...) are distinct AST
// variants, never `TSTypeReference`s, so they emit no sig (a `number` param
// carries no resolvable name); a union slot (`A | B`) emits one sig per arm.

/// One named type reference in a callable's signature. `owner` is the callable
/// node's span (the join key back to the `Node<TypeF>`); `ty` is the referenced
/// type's bare name, interned.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct TypeSig {
    pub owner: Span,
    pub slot: SigSlot,
    pub pos: u32,
    pub ty: NameId,
}

/// Where in a signature a `TypeSig` sits. `Param` = an input slot; `Ret` = the
/// output slot. (`Field`, for a class property's annotation, lands with the
/// class-fields pass; declared here is deferred to keep this commit's vocab to
/// what it emits.)
#[derive(Copy, Clone, Debug, PartialEq, Eq)]
pub enum SigSlot {
    Param,
    Ret,
}

impl SigSlot {
    pub fn as_str(self) -> &'static str {
        match self {
            SigSlot::Param => "param",
            SigSlot::Ret => "ret",
        }
    }
}

/// The TypeF side-channel: arrow-type sigs (and, later, consts/docs). Rides the
/// bundle's `aux`; the wire flattens it to `FlatFact::Sig`.
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct TypeFAux {
    pub sigs: Vec<TypeSig>,
}

impl Family for TypeF {
    type NodeKind = TypeEntityKind;
    type EdgeKind = TypeEdgeKind;
    type Aux = TypeFAux;
    const TAG: FamilyTag = FamilyTag::Type;
}

// ── RESOLUTION plane: CallF (commit 3a) ─────────────────────────────────────
//
// The call graph. The NODES are callable DEFINITIONS (the call facet of each
// declaration; the type facet is TypeF — two projections of one declaration,
// same spans, per the D-arrow-type ruling). Call SITES (the call expressions:
// `foo()`, `new Foo()`, `<Card/>`) are a side table of unresolved references
// (the callee as written); the caller is derived by span-containment at the
// seam. The resolved caller->callee EDGES land at `Resolve<CallF>` (commit 4).
#[derive(Default, Copy, Clone, Debug)]
pub struct CallF;

/// The call-def node: the callable's shape. v5 `CallKind`. `Lambda` (anonymous
/// callables surfaced by the df lift) lands with DfF.
#[derive(Copy, Clone, Debug, PartialEq, Eq)]
pub enum CallKind {
    /// A free function (top-level or nested named declaration, or a function
    /// bound to a `const`). The variant is `Free` (the callable shape); its WIRE
    /// tag is `"function"` — v5's `CallKind::Free.tag()` and `call_def.kind`, kept
    /// for byte-exact parity (a free function IS a function; v6's earlier `"free"`
    /// coinage had no rationale and broke the v5 diff).
    Free,
    /// A class method (incl. the constructor, whose call-name is the class name
    /// so a `new Foo()` site resolves to it).
    Method,
    /// An anonymous callable from the df lift (a closure value node). Emitted
    /// by the DfF pass, not this walk.
    Lambda,
}

impl CallKind {
    pub fn as_str(self) -> &'static str {
        match self {
            CallKind::Free => "function",
            CallKind::Method => "method",
            CallKind::Lambda => "lambda",
        }
    }
}

/// How a resolved call edge's callee was bound. v5's plain-text `call_edge` kind
/// distilled to the resolution provenance. Declared for the closed vocab;
/// emitted by `Resolve<CallF>` (commit 4).
#[derive(Copy, Clone, Debug, PartialEq, Eq)]
pub enum CallEdgeKind {
    /// The callee name resolved to exactly one def in the corpus.
    NameResolve,
    /// SCIP overrode the AST name-match (e.g. a call through a re-export).
    ScipOverride,
}

/// One call expression. `callee` is the trailing segment as written (the
/// resolution key); `callee_path` is the full qualified path when >1 segment
/// (`a.b.c()`), filled by resolution. v5 `ts_callee_name` collects the trailing
/// segment; the path is reconstructed at the seam.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct CallSite {
    pub span: Span,
    pub callee: NameId,
    pub callee_path: Option<NameId>,
}

/// The CallF side-channel: call sites (phase-1 unresolved call references).
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct CallFAux {
    pub sites: Vec<CallSite>,
}

impl Family for CallF {
    type NodeKind = CallKind;
    type EdgeKind = CallEdgeKind;
    type Aux = CallFAux;
    const TAG: FamilyTag = FamilyTag::Call;
}

// ── VALUE-FLOW plane: DfF (commit 3b) ───────────────────────────────────────
//
// Intra-procedural value flow: every value-bearing position in a callable's
// body is a NODE (a var read/write/bind, a param, a call result, a literal, a
// member access, a return, ...); local value flow is an EDGE. The two together
// are the dataflow graph the engine's `df_reaches` closure walks on the SAME
// fixpoint as the call/type/module graphs. This is the family SCIP CANNOT
// produce (no CFG/DDG in the format): always AST-backed, the differentiator.
//
// Identity is `(span, kind)`; the enclosing callable is NOT stored (derived at
// the seam by span-containment over the CallF defs, the same pattern as the
// CallF site caller). The enrichment aux (positional arg slots, field names,
// literal texts, loop/nest facts) lands in follow-ups; the EDGES already carry
// every value flow, so the graph is complete without them.

/// The df-node marker.
#[derive(Default, Copy, Clone, Debug)]
pub struct DfF;

/// df_node kind. v5 `decls.rs` brand, 23 variants. The TS walker (commit 3b)
/// emits the value-flow subset (param/let_bind/var_read/lit/call_res/new/member/
/// ret/binop/concat/template/cond/logic/closure/expr); the rest (borrow/unop/
/// loop/if/match/block/try/break/var_write) are Rust/other-lang kinds declared
/// now so the vocabulary is closed.
#[derive(Copy, Clone, Debug, PartialEq, Eq)]
pub enum DfNodeKind {
    Param,
    LetBind,
    VarRead,
    VarWrite,
    Lit,
    CallRes,
    New,
    Member,
    Ret,
    Borrow,
    Binop,
    Unop,
    Loop,
    If,
    Match,
    Block,
    Closure,
    Try,
    Break,
    Expr,
    // TS/JS flow additions (v5 ts flow.rs):
    Cond,
    Logic,
    Concat,
    Template,
}

impl DfNodeKind {
    pub fn as_str(self) -> &'static str {
        match self {
            DfNodeKind::Param => "param",
            DfNodeKind::LetBind => "let_bind",
            DfNodeKind::VarRead => "var_read",
            DfNodeKind::VarWrite => "var_write",
            DfNodeKind::Lit => "lit",
            DfNodeKind::CallRes => "call_res",
            DfNodeKind::New => "new",
            DfNodeKind::Member => "member",
            DfNodeKind::Ret => "ret",
            DfNodeKind::Borrow => "borrow",
            DfNodeKind::Binop => "binop",
            DfNodeKind::Unop => "unop",
            DfNodeKind::Loop => "loop",
            DfNodeKind::If => "if",
            DfNodeKind::Match => "match",
            DfNodeKind::Block => "block",
            DfNodeKind::Closure => "closure",
            DfNodeKind::Try => "try",
            DfNodeKind::Break => "break",
            DfNodeKind::Expr => "expr",
            DfNodeKind::Cond => "cond",
            DfNodeKind::Logic => "logic",
            DfNodeKind::Concat => "concat",
            DfNodeKind::Template => "template",
        }
    }
}

/// df_edge kind. v5's `df_edge(from, to)` was unkinded; `Direct` is that edge.
/// `Flow` (the promoted fifth family, std/flow.dl:89 — the interprocedural
// value-flow union: arg->param, ret->call_res, higher-order) lands with epic 5.
#[derive(Copy, Clone, Debug, PartialEq, Eq)]
pub enum DfEdgeKind {
    /// An intra-procedural value edge: `dst` receives the value of `src`.
    Direct,
}

impl DfEdgeKind {
    pub fn as_str(self) -> &'static str {
        match self {
            DfEdgeKind::Direct => "direct",
        }
    }
}

impl Family for DfF {
    type NodeKind = DfNodeKind;
    type EdgeKind = DfEdgeKind;
    type Aux = (); // enrichment (args/fields/lits/param_pos/loops) lands in follow-ups
    const TAG: FamilyTag = FamilyTag::Df;
}
