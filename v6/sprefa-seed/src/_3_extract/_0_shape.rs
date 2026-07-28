//! The normalized OUTPUT shape — one coordinate, one node, one edge.
//!
//! This file is the whole point of the v6 extraction crate. v5 had FOUR span
//! shapes (byte range / line-only / line+col-with-mixed-char-vs-byte / salted
//! WhereBytes), THREE `kind` representations (typed enums for type+call,
//! free-form `String` for df, `&'static str` for edges/modules), and SPLIT node
//! identity (`mint_sym` coordinate strings for type/call, dense `NodeIdx` for
//! df, kind-salted `WhereBytes` for the CST). That pile is the maintenance
//! nightmare and ~63% of the v5 dictionary bytes. It is deleted here, not
//! patched: a fact's "where" is ONE byte span, a fact's "what" is ONE typed
//! kind ordinal, a fact's identity is its span (never a minted string).
//!
//! Store coordination (see v6/sprefa-store/src/spine.rs): the store's `node`
//! table is `(node_id, family, file_id, byte_start, byte_len, kind, name_id?)`.
//! `RawNode` below is that row MINUS the store-assigned surrogate (`node_id`)
//! and with `file_id` implicit (the blob the extractor was handed). The engine
//! seam interns `RawNode -> node_id` by identity `(family, span, kind)`; extract
//! itself names no store id and no storage type (the crate-map boundary rail).
//!
//! All types here are content-LOCAL. `NameId` is extract's own arena interner;
//! the engine maps `NameId -> store::StrId` at the seam. `BlobHash` is the
//! content key (matches store `files.content_hash`, blake3 truncated to 16B).

/// A byte span into the blob the extractor was handed. THE coordinate. The
/// engine joins `span -> file_bytes -> files` to read text or derive line/col;
/// line/col are NEVER stored (v6 table-design D2; kills the 10.2%-of-dict
/// `file:line:col` class).
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, Hash)]
pub struct Span {
    pub start: u32,
    pub len: u32,
}

impl Span {
    pub const fn empty() -> Self { Self { start: 0, len: 0 } }
    /// Synthetic node identity for things with no real span (a whole-file
    /// module node, an anonymous param). Identity-stable, span-meaningless.
    pub const fn anchor(at: u32) -> Self { Self { start: at, len: 0 } }
}

/// Content key: blake3 truncated to 16 raw bytes (store `files.content_hash`).
/// Two byte-identical blobs anywhere in the corpus share ONE extraction. This
/// is the generalization of v5's per-file `FactCache<(repo,path,hash), _>`.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, Hash)]
pub struct BlobHash(pub [u8; 16]);

/// Extract's own arena-interned string id (a name, a variable, a specifier).
/// Dense u32 into the per-extraction arena string table. The engine seam interns
/// these into the store dictionary (`StrId`); extract never mints a qualified
/// `file::kind::name` sym (v5 `mint_sym`, 26.6% of dict bytes — deleted).
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, Hash)]
pub struct NameId(pub u32);

/// Local index into one file's node vec. Edges reference nodes by this during
/// extraction; the engine rewrites `NodeRef -> store::node_id` when it interns
/// the node vec (v5 `DfEdge { from: NodeIdx, to: NodeIdx }` generalized to all
/// families; v5 `TypeEdge { from: String }` syms deleted in favor of this).
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, Hash)]
pub struct NodeRef(pub u32);

// ── family ─────────────────────────────────────────────────────────────────
// Mirrors `store::spine::Family`. Duplicated here because extract sits BELOW
// store (it must not import the store crate); when the crates split, the two
// `Family` defs reconcile by a tiny shared-types leaf or a re-export. Four
// physical families stay four (index locality + independent cold-tier demand),
// never one giant `family`-discriminated table (v6 table-design D4).

/// The four static-analysis graph families extract produces.
#[derive(Copy, Clone, Debug, PartialEq, Eq, Hash)]
pub enum Family {
    /// Intra-procedural value flow: variables, params, calls, returns, lits.
    /// The only family SCIP cannot produce (no CFG/DDG in the format); always
    /// AST-backed.
    Df = 0,
    /// Inter-procedural call graph: defs + sites + resolved caller->callee edges.
    Call = 1,
    /// Type graph: declared entities + structural edges (field/variant/impl/...).
    Type = 2,
    /// Module graph: file->file import edges + per-binding side table.
    Module = 3,
}

// ── kind vocabularies ───────────────────────────────────────────────────────
// Each is a CLOSED enum stored as a small ordinal (i32 on the store), never a
// String and never an interned sym id. The variant sets are lifted verbatim
// from v5 `src/engine/decls.rs` `builtin_enum_brands()` — the closed brands the
// v5 typechecker already enforced. v5's free-form `DfNode.kind: String` (the one
// representation with no brand) is replaced by `DfNodeKind` here.

/// df_node kind. v5 `decls.rs:101-127` brand, 23 variants (incl. `break`).
#[derive(Copy, Clone, Debug, PartialEq, Eq, Hash)]
pub enum DfNodeKind {
    Param, LetBind, VarRead, VarWrite, Lit, CallRes, New, Member, Ret,
    Borrow, Binop, Unop, Loop, If, Match, Block, Closure, Try, Break, Expr,
    // TS/JS flow additions (v5 ts flow.rs):
    Cond, Logic, Concat, Template,
}

/// type_entity kind. v5 `decls.rs:84-96` brand, 9 variants.
/// (v5's `EntityKind` enum carried 11 incl Lambda+Module; the ENFORCED brand is
/// 9 — Lambda is call-family, Module is module-family. Normalized to the brand.)
#[derive(Copy, Clone, Debug, PartialEq, Eq, Hash)]
pub enum TypeEntityKind {
    Struct, Enum, Trait, Class, Interface, Alias, Function, Method, Const,
}

/// type_edge kind. v5 `decls.rs:77-81` brand, 7 variants.
#[derive(Copy, Clone, Debug, PartialEq, Eq, Hash)]
pub enum TypeEdgeKind {
    Field, Variant, Impl, Generic, Param, Returns, Uses,
}

/// call_def node kind (v5 `CallKind::tag`): the callable's shape.
#[derive(Copy, Clone, Debug, PartialEq, Eq, Hash)]
pub enum CallKind { Free, Method, Lambda }

/// How a call edge's callee was RESOLVED. v5 `call_edge` carried a plain-text
/// kind; the meaningful bit is the resolution provenance (single-def name match
/// vs SCIP override), so that is what the kind carries now.
#[derive(Copy, Clone, Debug, PartialEq, Eq, Hash)]
pub enum CallEdgeKind { NameResolve, ScipOverride }

/// module import edge kind (v5 `ModuleRef.kind`): the syntactic form.
#[derive(Copy, Clone, Debug, PartialEq, Eq, Hash)]
pub enum ModuleRefKind { Mod, Use, Import, SamePackage }

/// per-binding kind (v5 `module_binding.kind`): how a name enters scope.
#[derive(Copy, Clone, Debug, PartialEq, Eq, Hash)]
pub enum BindingKind { Named, Default, Namespace, SideEffect, Reexport }

/// const-value / literal kind (v5 `const_value_kind`).
#[derive(Copy, Clone, Debug, PartialEq, Eq, Hash)]
pub enum ConstValueKind { Lit, Template, Concat }

/// flow_edge kind — the FIFTH family v5 half-built in `std/flow.dl:89`: the
/// interprocedural value-flow graph that unions df_edge with the positional
/// arg->param hop and the ret->call_res backward hop over call_edge. v5 left it
/// in a stdlib .dl; v6 promotes it to a first-class edge kind so the union is
/// typed, not stringly-joined. (k-CFA per-site cloning + node-level types stay
/// frontier — v5 stated both out of scope.)
#[derive(Copy, Clone, Debug, PartialEq, Eq, Hash)]
pub enum FlowEdgeKind {
    DfDirect,        // an intra-procedural df_edge, carried through
    ArgToParam,      // call-site arg -> callee param (positional, via df_arg/df_param)
    RetToCallRes,    // callee ret -> call-site call_res node (backward hop)
    LambdaElem,      // higher-order: element flows into a lambda param
    LambdaRet,       // higher-order: lambda result flows out
}

/// The typed node kind — one sum over the four families. Its ordinal is what
/// the store's `node.kind` column holds; `family` disambiguates the namespace.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub enum NodeKind {
    Df(DfNodeKind),
    Type(TypeEntityKind),
    Call(CallKind),
    Module(ModuleNodeKind),
}

/// Module nodes are file-keyed and largely implicit (a file IS the module
/// node). The kind distinguishes a real file node from a synthetic package root.
#[derive(Copy, Clone, Debug, PartialEq, Eq, Hash)]
pub enum ModuleNodeKind { File, PackageRoot }

/// The typed edge kind. `Df` is v5's unkinded `df_edge(from,to)` (one variant);
/// `Module` is v5's unkinded `module_edge(src,dst)` plus the binding side table.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub enum EdgeKind {
    Df,                          // intra-procedural value edge (unkinded in v5)
    Call(CallEdgeKind),
    Type(TypeEdgeKind),
    Module(ModuleRefKind),
    Flow(FlowEdgeKind),          // the promoted fifth family
}

impl NodeKind {
    /// Which family this node belongs to (drives which physical table it lands in).
    pub fn family(self) -> Family {
        match self {
            NodeKind::Df(_) => Family::Df,
            NodeKind::Type(_) => Family::Type,
            NodeKind::Call(_) => Family::Call,
            NodeKind::Module(_) => Family::Module,
        }
    }
}

impl EdgeKind {
    pub fn family(self) -> Family {
        match self {
            EdgeKind::Df | EdgeKind::Flow(_) => Family::Df, // flow edges live on the df plane
            EdgeKind::Call(_) => Family::Call,
            EdgeKind::Type(_) => Family::Type,
            EdgeKind::Module(_) => Family::Module,
        }
    }
}

// ── the output rows ─────────────────────────────────────────────────────────

/// ONE located, kinded thing in a file: a df node, a type entity, a callable
/// def, a module file. Identity = `(family, span, kind)` (the store's
/// `ux_node_identity`). `name` is the optional bare/qualified identifier for
/// resolution joins — it is NOT the identity (that was v5's `mint_sym` disease).
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct RawNode {
    pub span: Span,
    pub kind: NodeKind,
    pub name: Option<NameId>,
}

/// ONE resolved relationship between two nodes in the same extraction. `src`
/// and `dst` are local `NodeRef`s into the producing file's node vec (intra-file
/// edges) OR cross-file refs resolved at the project phase (see `ProjectEdge`).
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct RawEdge {
    pub src: NodeRef,
    pub dst: NodeRef,
    pub kind: EdgeKind,
}

/// A project-phase edge: `dst` lives in ANOTHER blob (resolved across the file
/// set). v5 modeled this by smuggling the target into a `Resolution` enum or a
/// qualified sym string; here it is a content key + the target's span, resolved
/// to a store `node_id` at the seam by joining `(blob, span, kind)`.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct ProjectEdge {
    pub src: NodeRef,                 // local node in this file
    pub dst_blob: BlobHash,           // the resolved target's content
    pub dst_span: Span,               // the target node's coordinate
    pub kind: EdgeKind,
}

/// v5 `Resolution { File | External | Unresolved }`, content-keyed. The project
/// phase turns a `specifier` (phase 1) into one of these; only `Local` yields a
/// `ProjectEdge`, the other two become edge `kind` metadata (External/Unresolved).
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum Resolution {
    Local { blob: BlobHash, span: Span },
    External(NameId),   // a dependency outside the corpus
    Unresolved(NameId), // specifier nothing resolved to
}
