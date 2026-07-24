//! THE canonical type module for the sprefa-extract leaf.
//!
//! Every public type / trait / enum / struct / impl lives here. The other modules
//! (shape, family, rows, seams, source) are `pub use crate::types::*` re-export
//! shims so historical import paths (`crate::shape::Span`, `crate::family::TypeF`,
//! ...) keep resolving. This is the "tasks.rs technique" from the seed, promoted:
//! one compiled file is the source of truth, and pending work is COMMENTED OUT
//! (Resolve<F>, ModuleF, Flow edges, GoSource).
//!
//! Leaf scope: a corpus at a version -> normalized graph facts. Pure CPU, no SQL,
//! no datalog, no async (the engine, another worktree).
//!
//! Planes:  RESOLUTION (SCIP-wire): CallF, TypeF, ModuleF*
//!          VALUE-FLOW (native):   DfF  (+ typed Flow* edges)
//!          STRUCTURE (lossless):  CstF        (* = pending, commented out)

use std::fmt;
use std::marker::PhantomData;

use serde::Serialize;

// ════════════════════════════════════════════════════════════════════════════
// S1 ATOMS
// ════════════════════════════════════════════════════════════════════════════

/// THE one coordinate. Byte offsets into the file; line/col derived, never stored.
#[derive(Copy, Clone, Debug, Default, PartialEq, Eq, Hash)]
pub struct Span {
    pub start: u32,
    pub len: u32,
}

impl Span {
    pub const fn empty() -> Self {
        Self { start: 0, len: 0 }
    }
    /// Synthetic identity for things with no real span (a whole-file module).
    pub const fn anchor(at: u32) -> Self {
        Self { start: at, len: 0 }
    }
    pub const fn end(self) -> u32 {
        self.start + self.len
    }
}

/// Content key: blake3 truncated to 16 raw bytes (store `files.content_hash`).
/// Declared here; the hash is NOT computed yet (the cache lands with BlobSource).
#[derive(Copy, Clone, Debug, Default, PartialEq, Eq, Hash)]
pub struct BlobHash(pub [u8; 16]);

/// Dense u32 into the per-file `Strings` interner.
#[derive(Copy, Clone, Debug, Default, PartialEq, Eq, Hash)]
pub struct NameId(pub u32);

/// Local index into one file's node vec; flattened to a span at the wire.
#[derive(Copy, Clone, Debug, Default, PartialEq, Eq, Hash)]
pub struct NodeRef(pub u32);

/// The flat family discriminant at the seam only (the wire, the ratchet key).
#[derive(Copy, Clone, Debug, PartialEq, Eq, Hash, Serialize, serde::Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum FamilyTag {
    Df,
    Call,
    Type,
    Module,
    Cst,
}

/// The per-file string interner backing every `NameId`. One per extraction; the
/// dispatch creates it, passes `&mut` to each projector, keeps it so the wire
/// flatten can resolve `NameId -> &str`. Dedups on insert.
#[derive(Default)]
pub struct Strings {
    map: std::collections::HashMap<String, NameId>,
    names: Vec<String>,
}

impl Strings {
    pub fn new() -> Self {
        Self::default()
    }

    /// Intern `s`, returning a stable `NameId`. Byte-identical strings share one id.
    pub fn intern(&mut self, s: &str) -> NameId {
        if let Some(&id) = self.map.get(s) {
            return id;
        }
        let id = NameId(self.names.len() as u32);
        self.map.insert(s.to_string(), id);
        self.names.push(s.to_string());
        id
    }

    pub fn lookup(&self, id: NameId) -> &str {
        &self.names[id.0 as usize]
    }

    pub fn len(&self) -> usize {
        self.names.len()
    }

    pub fn is_empty(&self) -> bool {
        self.names.is_empty()
    }
}

impl fmt::Display for NameId {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "NameId({})", self.0)
    }
}

// ════════════════════════════════════════════════════════════════════════════
// S2 FAMILY MODEL
// ════════════════════════════════════════════════════════════════════════════

/// One static-analysis family. The associated kinds are the per-family node and
/// edge vocabularies; `TAG` is the flat discriminant used at the seam only. `Aux`
/// is the family's side-channel payload (TypeF sigs/consts, CallF sites): per-
/// node/per-occurrence attributes that are NOT span-pair edges and do not fit the
/// uniform `Node<F>`/`Edge<F>` shape. The bundle carries one `F::Aux`; the wire
/// flattens it to its own `FlatFact` arm.
pub trait Family {
    type NodeKind: Clone + fmt::Debug;
    type EdgeKind: Copy + Clone + fmt::Debug;
    type Aux: Default + Clone + fmt::Debug;
    const TAG: FamilyTag;
}

// ── STRUCTURE plane: CstF ───────────────────────────────────────────────────

/// The lossless named-node tree (the tree-sitter CST, via ast-grep's grammars).
/// `NodeKind` is an OPEN grammar vocabulary interned as a NameId
/// (`function_declaration`, ...); not a closed enum. The single edge kind is Child.
#[derive(Default, Copy, Clone, Debug)]
pub struct CstF;

/// src = parent named node, dst = child named node. Unnamed punctuation nodes are
/// not emitted; their named descendants reparent to the nearest named ancestor.
#[derive(Copy, Clone, Debug, PartialEq, Eq)]
pub enum CstEdgeKind {
    Child,
}

impl Family for CstF {
    type NodeKind = NameId;
    type EdgeKind = CstEdgeKind;
    type Aux = ();
    const TAG: FamilyTag = FamilyTag::Cst;
}

// ── RESOLUTION plane: TypeF ─────────────────────────────────────────────────

/// The type graph: declared entities (class/interface/alias/enum/function/method/
/// struct/trait/const) + their structural edges. Entity NODES ship in phase 1; the
/// type EDGES (field/impl/uses/...) land with Resolve<TypeF>.
#[derive(Default, Copy, Clone, Debug)]
pub struct TypeF;

/// type_entity kind. 9 variants. Struct/Trait are Rust-only; TS emits the rest.
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

/// type_edge kind. 7 variants. Declared to close the vocabulary; emitted ONLY by
/// Resolve<TypeF>.
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

/// One named type reference in a callable's signature. `owner` = the callable
/// node's span (join key); `ty` = the referenced type's bare name (unresolved in
/// phase 1; Resolve<TypeF> binds it). `pos` preserves param order.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct TypeSig {
    pub owner: Span,
    pub slot: SigSlot,
    pub pos: u32,
    pub ty: NameId,
}

/// Where in a signature a TypeSig sits. Param = input slot; Ret = output slot.
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

/// One resolved string folded from a `const`/`as const` binding (or string-enum
/// member). `owner` joins to the Const/Enum entity; `field` is None for a bare
/// const, else a dotted path / enum member; `text` is the value; `kind` is lit
/// (cooked) or template (raw slice, holes intact).
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct ConstValue {
    pub owner: Span,
    pub field: Option<NameId>,
    pub text: NameId,
    pub kind: ConstKind,
}

#[derive(Copy, Clone, Debug, PartialEq, Eq)]
pub enum ConstKind {
    Lit,
    Template,
}

impl ConstKind {
    pub fn as_str(self) -> &'static str {
        match self {
            ConstKind::Lit => "lit",
            ConstKind::Template => "template",
        }
    }
}

/// The TypeF side-channel: arrow-type sigs + the const facet.
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct TypeFAux {
    pub sigs: Vec<TypeSig>,
    pub consts: Vec<ConstValue>,
}

impl Family for TypeF {
    type NodeKind = TypeEntityKind;
    type EdgeKind = TypeEdgeKind;
    type Aux = TypeFAux;
    const TAG: FamilyTag = FamilyTag::Type;
}

// ── RESOLUTION plane: CallF ─────────────────────────────────────────────────

/// The call graph. NODES are callable definitions (the call facet of a
/// declaration; TypeF is its type facet, same spans). SITES are unresolved call
/// references; the caller is derived by span-containment at the seam. Resolved
/// caller->callee edges land with Resolve<CallF>.
#[derive(Default, Copy, Clone, Debug)]
pub struct CallF;

/// The call-def node shape. `Free` wires as "function" (v5 parity).
#[derive(Copy, Clone, Debug, PartialEq, Eq)]
pub enum CallKind {
    /// A free function. Wire tag is "function" (v5 CallKind::Free.tag()).
    Free,
    /// A class method (incl. the constructor).
    Method,
    /// An anonymous callable from the df lift (emitted by the DfF pass).
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

/// How a resolved call edge's callee was bound. Emitted by Resolve<CallF>.
#[derive(Copy, Clone, Debug, PartialEq, Eq)]
pub enum CallEdgeKind {
    /// The callee name resolved to exactly one def in the corpus.
    NameResolve,
    /// SCIP overrode the AST name-match.
    ScipOverride,
}

/// One call expression. `callee` = trailing segment as written (resolution key);
/// `callee_path` = full qualified path when >1 segment (filled by resolution).
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct CallSite {
    pub span: Span,
    pub callee: NameId,
    pub callee_path: Option<NameId>,
}

/// The CallF side-channel: call sites (phase-1 unresolved).
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

// ── VALUE-FLOW plane: DfF ───────────────────────────────────────────────────

/// Intra-procedural value flow: every value-bearing position is a NODE; local
/// value flow is a Direct EDGE. Always AST-backed (SCIP cannot produce this).
#[derive(Default, Copy, Clone, Debug)]
pub struct DfF;

/// df_node kind. 23 variants.
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

/// df_edge kind. `Direct` is v5's unkinded df_edge(from,to). `Flow` (the promoted
/// interprocedural union: arg->param, ret->call_res, higher-order) is PENDING.
#[derive(Copy, Clone, Debug, PartialEq, Eq)]
pub enum DfEdgeKind {
    /// An intra-procedural value edge: dst receives the value of src.
    Direct,
    // Flow(FlowEdgeKind), // PENDING epic 5
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

// ── RESOLUTION plane: ModuleF  (PENDING - collapsed; not yet a family) ──────
// The resolution half folds into SCIP namespace edges (a file IS a namespace);
// the binding half into aux metadata. Whether it becomes a standalone Family is
// undecided. Sketch:
//
// #[derive(Default, Copy, Clone, Debug)]
// pub struct ModuleF;
// pub enum ModuleNode { File, PkgRoot }
// pub enum ModuleEdge { Declares, ReExports, Imports }
// impl Family for ModuleF {
//     type NodeKind = ModuleNode; type EdgeKind = ModuleEdge; type Aux = ();
//     const TAG: FamilyTag = FamilyTag::Module;
// }

// ════════════════════════════════════════════════════════════════════════════
// S3 ROWS
// ════════════════════════════════════════════════════════════════════════════

/// One located, kinded thing in a file. Identity = (family, span, kind); `name`
/// is the optional bare identifier for resolution joins, NOT the identity.
#[derive(Clone, Debug)]
pub struct Node<F: Family> {
    pub span: Span,
    pub kind: F::NodeKind,
    pub name: Option<NameId>,
    _f: PhantomData<fn() -> F>,
}

impl<F: Family> Node<F> {
    pub fn new(span: Span, kind: F::NodeKind) -> Self {
        Self { span, kind, name: None, _f: PhantomData }
    }

    pub fn with_name(mut self, name: NameId) -> Self {
        self.name = Some(name);
        self
    }
}

/// One resolved relationship between two nodes in the same file. `src`/`dst` are
/// local NodeRefs into the producing file's node vec; both flatten to spans.
#[derive(Clone, Copy, Debug)]
pub struct Edge<F: Family> {
    pub src: NodeRef,
    pub dst: NodeRef,
    pub kind: F::EdgeKind,
    _f: PhantomData<fn() -> F>,
}

impl<F: Family> Edge<F> {
    pub fn new(src: NodeRef, dst: NodeRef, kind: F::EdgeKind) -> Self {
        Self { src, dst, kind, _f: PhantomData }
    }
}

/// One family's output for one file.
#[derive(Clone, Debug)]
pub struct FamilyBundle<F: Family> {
    pub nodes: Vec<Node<F>>,
    pub edges: Vec<Edge<F>>,
    pub aux: F::Aux,
}

impl<F: Family> Default for FamilyBundle<F> {
    fn default() -> Self {
        Self { nodes: Vec::new(), edges: Vec::new(), aux: F::Aux::default() }
    }
}

impl<F: Family> FamilyBundle<F> {
    pub fn node(&self, r: NodeRef) -> &Node<F> {
        &self.nodes[r.0 as usize]
    }
}

// ════════════════════════════════════════════════════════════════════════════
// S5 SEAMS
// ════════════════════════════════════════════════════════════════════════════

/// Why a parse failed.
#[derive(Debug)]
pub enum ParseError {
    NoGrammar(String),
    Utf8(String),
    Parse(String),
}

impl fmt::Display for ParseError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            ParseError::NoGrammar(path) => write!(f, "no grammar for {path}"),
            ParseError::Utf8(msg) => write!(f, "source is not valid UTF-8: {msg}"),
            ParseError::Parse(msg) => write!(f, "parser failed: {msg}"),
        }
    }
}

impl std::error::Error for ParseError {}

/// content -> parsed CST handle. One impl per backing engine. The arena is
/// caller-owned (lent to parse) because some engines borrow their backing store
/// (oxc's Program<'a> borrows its Allocator; ast-grep sets Arena = ()).
pub trait Parser: Sync + Send {
    type Arena;
    type Parsed<'a>
    where
        Self: 'a;

    fn name(&self) -> &'static str;
    fn matches(&self, path: &str) -> bool;
    fn make_arena(&self) -> Self::Arena;
    fn parse<'a>(
        &self,
        arena: &'a Self::Arena,
        path: &str,
        content: &'a [u8],
    ) -> Result<Self::Parsed<'a>, ParseError>;
}

/// Phase 1: one parse, masked projections. Interns names/kinds into `strings`
/// and pushes rows into `sink`.
pub trait Project<F: Family>: Sync + Send {
    type Parsed<'a>;
    fn project<'a>(
        &self,
        parsed: &Self::Parsed<'a>,
        strings: &mut Strings,
        sink: &mut FamilyBundle<F>,
    );
}

/// File bytes in, content-hashed out. SOURCE-AGNOSTIC (git worktree OR plain
/// directory OR ...). Trait only; impls + the cache are PENDING.
pub trait BlobSource: Sync + Send {
    fn blob(&self, path: &str) -> Option<Vec<u8>>;
}

// pub trait Resolve<F: Family>: Sync + Send {                                  // PENDING commit 4
//     /// Phase 2: name-resolved edges over one file's bundle + the project context
//     /// (all files' declared names). df/cst have no phase 2.
//     ///   Resolve<TypeF>: field/impl/variant/uses/generic + resolved param/returns.
//     ///   Resolve<CallF>: resolved caller -> callee (the call_site join).
//     fn resolve(&self, bundle: &FamilyBundle<F>, cx: &ProjectCx) -> Vec<ProjectEdge>;
// }

// ════════════════════════════════════════════════════════════════════════════
// UNIFORM SURFACE
// ════════════════════════════════════════════════════════════════════════════

/// Which families to extract; the Source projects only the masked ones.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub struct FamilyMask {
    pub cst: bool,
    pub types: bool,
    pub call: bool,
    pub df: bool,
}

impl FamilyMask {
    pub const ALL: Self = Self { cst: true, types: true, call: true, df: true };
    pub const NONE: Self = Self { cst: false, types: false, call: false, df: false };
}

/// One file's extraction: the shared per-file interner + an Option<FamilyBundle<F>>
/// per family. Sharing ONE Strings is byte-stable (flatten resolves NameId -> &str).
#[derive(Default)]
pub struct ExtractOutput {
    pub strings: Strings,
    pub cst: Option<FamilyBundle<CstF>>,
    pub types: Option<FamilyBundle<TypeF>>,
    pub call: Option<FamilyBundle<CallF>>,
    pub df: Option<FamilyBundle<DfF>>,
}

/// One language binding: a Parser + its per-family Project<F>s behind one masked
/// extract. The v5 TypeLang analog. Held &'static in the roster; no mutable state.
pub trait Source: Sync + Send {
    fn name(&self) -> &'static str;
    fn matches(&self, path: &str) -> bool;
    /// One parse per backing engine, masked projections. Owns the arena(s)
    /// internally; returns owned output (no borrowed parse crosses the seam).
    fn extract(&self, path: &str, content: &[u8], mask: FamilyMask) -> ExtractOutput;
}

// ════════════════════════════════════════════════════════════════════════════
// WIRE TYPES  (the flat tagged envelope; serde lives here, NOT on Node<F>)
// ════════════════════════════════════════════════════════════════════════════

/// A span on the wire: inclusive-exclusive byte offsets into the file.
#[derive(Copy, Clone, Debug, Serialize)]
pub struct SpanOut {
    pub start: u32,
    pub end: u32,
}

impl SpanOut {
    pub fn new(start: u32, end: u32) -> Self {
        Self { start, end }
    }
}

/// One flat fact. The `record` tag discriminates the shape; `family` carries the
/// plane. Serialized as JSONL (`{"record":"node",...}` etc.).
#[derive(Serialize, Debug)]
#[serde(tag = "record", rename_all = "lowercase")]
pub enum FlatFact {
    Node {
        family: FamilyTag,
        span: SpanOut,
        kind: String,
        name: Option<String>,
    },
    Edge {
        family: FamilyTag,
        kind: String,
        from: SpanOut,
        to: SpanOut,
    },
    /// TypeF arrow-type sig: owner = callable span, slot = param/ret, pos, ty.
    Sig {
        family: FamilyTag,
        owner: SpanOut,
        slot: String,
        pos: u32,
        ty: String,
    },
    /// CallF call site (phase-1 unresolved): span, callee as written, optional path.
    Site {
        family: FamilyTag,
        span: SpanOut,
        callee: String,
        callee_path: Option<String>,
    },
    /// TypeF const value: owner, optional field path, text, kind = lit|template.
    Const {
        family: FamilyTag,
        owner: SpanOut,
        field: Option<String>,
        text: String,
        kind: String,
    },
}

// flatten / flatten_jsonl live in wire.rs (the logic, not the types).

// ════════════════════════════════════════════════════════════════════════════
// LANGUAGE ROSTER  (lang/mod.rs) - first-match by extension
// ════════════════════════════════════════════════════════════════════════════
// Each Source = cst via ast-grep + type/call/df/const via a native front-end.
// A new language = ONE file + one roster line + one fixture.
//
// pub fn sources() -> &'static [&'static dyn Source] {
//     &[
//         &RustSource,    //  .rs               syn front-end            (lang/rust.rs)
//         &GoSource,      //  .go               tree-sitter-go           (lang/go.rs)  // PENDING (in flight)
//         &TsSource,      //  .ts/.tsx/.js/...  oxc front-end            (lang/ts.rs)
//         &AstgrepSource, //  fallback: cst-only for any ast-grep grammar
//     ]
// }

// ════════════════════════════════════════════════════════════════════════════
// STATUS  (flip a cell when it ships; [x] = ported + parity-green)
// ════════════════════════════════════════════════════════════════════════════
//
//                          TS (oxc)   Rust (syn)   Go (tree-sitter-go)
//   cst (ast-grep)           [x]         [x]            [x]
//   type entities + sigs     [x]         [x]            [x]
//   const facet              [x]         [x]            [-] n/a (v5 go emits none)
//   call defs + sites        [x]         [x]            [x]
//   df nodes + edges         [x]         [x]            [x]
//   parity vs v5 oracle      [x]         [x] *          [x]
//
//   * rust parity: one self-verifying closure-df-node-name waiver.
//
// DEFERRED FOR ALL LANGS (gated on Resolve<F> commit 4, or follow-ups):
//   type_edge (field/impl/variant/uses/generic)   -> Resolve<TypeF>
//   resolved caller -> callee                     -> Resolve<CallF>
//   docs facet                                    -> follow-up
//   df aux (args/fields/lits/param_pos)           -> labels, follow-up
//
// LEAF INFRA (pure CPU; still this leaf): parallel dispatch (rayon, arena-per-
//   worker); BlobSource impls + the (BlobHash, lang, mask) content-keyed cache.
//
// OUT OF SCOPE (engine, another worktree): store-seam wiring (seed Extract trait
//   is todo!()), datalog fixpoint, reactivity, async-eval.
