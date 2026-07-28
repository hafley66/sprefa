//! THE canonical type module for the sprefa-extract leaf.
//!
//! Every public type / trait / enum / struct / impl lives here. The other modules
//! (shape, family, rows, seams, source) are `pub use crate::types::*` re-export
//! shims so historical import paths (`crate::shape::Span`, `crate::family::TypeF`,
//! ...) keep resolving. This is the "tasks.rs technique" from the seed, promoted:
//! one compiled file is the source of truth, and pending work is COMMENTED OUT
//! (ModuleF, Flow edges). Resolve<F> landed in commit 4a as a HOLLOW design-freeze
//! surface (S5 below): the trait + ProjectCx + ProjectEdge<F> exist, bodies are
//! todo!(), and NOTHING calls resolve yet. Commit 4b-i fills the shared machinery
//! (BlobHash::of, the DefIndex builder + the three pure lookup helpers, the
//! FlatFact project-edge arm); the Resolve impls themselves land per-lang.
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
#[derive(Copy, Clone, Debug, Default, PartialEq, Eq, PartialOrd, Ord, Hash)]
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
/// Computed by `BlobHash::of` (commit 4b-i; the human-approved blake3 dep); the
/// (BlobHash, lang, mask) phase-1 cache that consumes it still lands with
/// BlobSource.
#[derive(Copy, Clone, Debug, Default, PartialEq, Eq, Hash)]
pub struct BlobHash(pub [u8; 16]);

impl BlobHash {
    /// Hash file bytes: blake3's 32-byte digest truncated to the first 16.
    pub fn of(content: &[u8]) -> Self {
        let digest = blake3::hash(content);
        let mut bytes = [0u8; 16];
        bytes.copy_from_slice(&digest.as_bytes()[..16]);
        Self(bytes)
    }

    /// Lowercase hex (32 chars) — the wire form of the content key.
    pub fn to_hex(self) -> String {
        let mut out = String::with_capacity(32);
        for b in self.0 {
            out.push_str(&format!("{b:02x}"));
        }
        out
    }
}

/// A digest of the file set that affects resolution (which files exist + their
/// manifest membership), folded from the corpus so two identical blobs in
/// identical file-set contexts share phase-2 work. The middle component of the
/// phase-2 cache key (see `Resolve`). Spec: seed `_1_mask.rs`:78-82. Declared
/// here; NOT computed yet (lands with the phase-2 cache, same as BlobHash).
#[derive(Copy, Clone, Debug, Default, PartialEq, Eq, Hash)]
pub struct ProjectDigest(pub [u8; 16]);

/// Dense u32 into the per-file `Strings` interner.
#[derive(Copy, Clone, Debug, Default, PartialEq, Eq, PartialOrd, Ord, Hash)]
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
/// type EDGES (field/impl/uses/...) are phase-2 `Resolve<TypeF>` output, bound
/// from the phase-1 candidate rows (4b-iii; see `TypeEdgeCandidate`).
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

/// type_edge kind. 7 variants. Rides the phase-1 candidate row (unresolved) and
/// the `ProjectEdge` (resolved) — the edges themselves are emitted ONLY by
/// `Resolve<TypeF>`.
#[derive(Copy, Clone, Debug, PartialEq, Eq, PartialOrd, Ord)]
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

/// One UNRESOLVED type-edge candidate: `owner` = the owning entity's span (the
/// TypeF node join key), `to` = the referenced name AS WRITTEN — including v5's
/// synthetic `Owner::Member` variant text, which names no node — and `kind`.
/// USER RULING (2026-07-24, option (a)): collected in PHASE 1 during the one
/// parse, exactly the `CallFAux.specifiers` pattern, so the resolve arm binds
/// purely with zero AST. RESOLVE INPUT ONLY: no FlatFact arm (the 4a wire
/// ruling stands — the wire carries the resolved `ProjectEdge`, never the
/// candidate). The candidate row IS the parity target: v5's `type_edge.to` is
/// free text (decls.rs:517), so text dsts STAY text — the twin-normalize reads
/// this row's owner/to/kind, never a node join.
#[derive(Clone, Debug, PartialEq, Eq, PartialOrd, Ord)]
pub struct TypeEdgeCandidate {
    pub owner: Span,
    pub to: NameId,
    pub kind: TypeEdgeKind,
}

/// The TypeF side-channel: arrow-type sigs + the const facet + the unresolved
/// type-edge candidates (4b-iii).
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct TypeFAux {
    pub sigs: Vec<TypeSig>,
    pub consts: Vec<ConstValue>,
    pub candidates: Vec<TypeEdgeCandidate>,
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
/// ADDENDUM 4a (site-key discipline): method resolution is NAME-ONLY — the
/// callee name binds via the corpus `DefIndex` (`NameResolve`) and SCIP may
/// override that binding (`ScipOverride`). Receiver typing (type-of-receiver ->
/// method set) is OUT OF SCOPE for commit 4: no lang resolve arm invents it.
#[derive(Copy, Clone, Debug, PartialEq, Eq)]
pub enum CallEdgeKind {
    /// The callee name resolved to exactly one def in the corpus.
    NameResolve,
    /// SCIP overrode the AST name-match.
    ScipOverride,
}

impl CallEdgeKind {
    pub fn as_str(self) -> &'static str {
        match self {
            CallEdgeKind::NameResolve => "name_resolve",
            CallEdgeKind::ScipOverride => "scip_override",
        }
    }
}

/// One call expression. `callee` = trailing segment as written (resolution key);
/// `callee_path` = full qualified path when >1 segment (filled by resolution).
/// ADDENDUM 4a (site-key discipline): `callee_path` is collected UNIFORMLY at
/// phase 1 — every lang fills it for multi-segment paths as written (rust
/// already does; ts/go emit None today and catch up with their resolve arms) —
/// so no resolve arm re-derives path text from its own AST.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct CallSite {
    pub span: Span,
    pub callee: NameId,
    pub callee_path: Option<NameId>,
}

/// A phase-1 module specifier (import / use / from / require), AS WRITTEN — the
/// resolution input the Resolve<TypeF>/Resolve<CallF> arms bind through.
/// ADDENDUM 4a: the row shape ONLY is declared (no lang emission code) so 4b+
/// collects these rows in PHASE 1; without phase-1 rows every resolve arm
/// would re-walk its own AST in phase 2 — the triplication the phase split
/// exists to prevent. `name` is the specifier text as written (the bound name;
/// the module path for path-only forms like go's imports). The seed's fuller
/// `Binding` side table (local / source / imported, `_1_mask.rs`:67-76) is the
/// 4b evolution path if TS's from-clause needs a separate source field —
/// FLAGGED for human review. 4b-ii: `TsSource` collects these (see lang/ts.rs
/// `module_specifiers`); the from-module field was NOT needed yet (nothing
/// consumes specifiers before Resolve<CallF>), so it stays unadded.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Specifier {
    pub span: Span,
    pub name: NameId,
    pub kind: SpecifierKind,
}

/// How the name enters scope. The seed's `BindingKind` vocabulary
/// (`_0_shape.rs`:127-129; v5 `module_binding.kind`), renamed for the row.
#[derive(Copy, Clone, Debug, PartialEq, Eq)]
pub enum SpecifierKind {
    Named,
    Default,
    Namespace,
    SideEffect,
    Reexport,
}

impl SpecifierKind {
    pub fn as_str(self) -> &'static str {
        match self {
            SpecifierKind::Named => "named",
            SpecifierKind::Default => "default",
            SpecifierKind::Namespace => "namespace",
            SpecifierKind::SideEffect => "side_effect",
            SpecifierKind::Reexport => "reexport",
        }
    }
}

/// The CallF side-channel: call sites + module specifiers (both phase-1
/// unresolved). ADDENDUM 4a RULING (flagged for human review): specifier rows
/// live HERE, on the existing CallF aux — NOT on a revived ModuleF (D-module:
/// the binding half is aux side metadata, not a standalone resolution family)
/// and NOT on ExtractOutput (a new field there would break the four lang
/// files' exhaustive `ExtractOutput { .. }` literals). Resolve arms of BOTH
/// families read them: `resolve` takes the whole `ExtractOutput`, and any
/// resolution run masks call+types anyway (the `DefIndex` is built from both
/// families' output).
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct CallFAux {
    pub sites: Vec<CallSite>,
    pub specifiers: Vec<Specifier>,
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
// undecided. Per spec `_2_traits.rs`:80-84 module RESOLVES once it lands; the
// 4a Resolve surface (S5 below) deliberately declares NO ModuleF arm - see the
// `Resolve` trait doc. ADDENDUM 4a RULING: phase-1 specifier rows live in
// `CallFAux.specifiers` (the binding half as aux side metadata, exactly as
// D-module says); ModuleF stays collapsed. Flagged for human review; revival
// stays possible. Sketch:
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
        Self {
            span,
            kind,
            name: None,
            _f: PhantomData,
        }
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
        Self {
            src,
            dst,
            kind,
            _f: PhantomData,
        }
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
        Self {
            nodes: Vec::new(),
            edges: Vec::new(),
            aux: F::Aux::default(),
        }
    }
}

impl<F: Family> FamilyBundle<F> {
    pub fn node(&self, r: NodeRef) -> &Node<F> {
        &self.nodes[r.0 as usize]
    }
}

/// A project-phase edge: `dst` lives in ANOTHER blob (resolved across the file
/// set). The seed's `ProjectEdge` (`_0_shape.rs`:222-232) made generic over the
/// family — the seed's `EdgeKind` sum is deleted per D-families, so `kind` is
/// `F::EdgeKind`. Emitted ONLY by `Resolve<F>` (phase 2); the store seam
/// resolves the dst to a `node_id` by joining `(dst_blob, dst_span, kind)`.
#[derive(Clone, Copy, Debug)]
pub struct ProjectEdge<F: Family> {
    /// Local node in this file (into the producing file's node vec).
    pub src: NodeRef,
    /// The resolved target's content key.
    pub dst_blob: BlobHash,
    /// The target node's coordinate inside `dst_blob`.
    pub dst_span: Span,
    pub kind: F::EdgeKind,
    _f: PhantomData<fn() -> F>,
}

impl<F: Family> ProjectEdge<F> {
    pub fn new(src: NodeRef, dst_blob: BlobHash, dst_span: Span, kind: F::EdgeKind) -> Self {
        Self {
            src,
            dst_blob,
            dst_span,
            kind,
            _f: PhantomData,
        }
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

// ── phase 2: the Resolve seam (commit 4a DESIGN FREEZE) ─────────────────────
// The trait surface + types only. Every method body is todo!(); NOTHING calls
// resolve; no impl exists yet (per-family impls land in 4b/4c). Human review
// gates 4b.

/// Borrowed view over one (repo, rev) project, shared across a language's
/// phase-2 calls. Extract is content-local; this is the ONLY handle it gets to
/// the world beyond the blob it was handed. Spec: seed `_2_traits.rs`:29-51
/// (per-field citations below). Every field is a HOLLOW declaration in 4a: the
/// concrete FileSet / ManifestMap / IndexBag subsystems land with the phase-2
/// implementations that need them, not before.
pub struct ProjectCx<'a> {
    /// Project-relative tracked file set (the resolution universe; a specifier
    /// resolving outside it is External/Unresolved). Spec: `_2_traits.rs`:36-37
    /// (field) + :53-56 (FileSet).
    pub files: &'a FileSet,
    /// Manifest path -> contents (Cargo.toml / package.json / go.mod); feeds the
    /// per-language package indexes (RustCrates / ts_packages / GoIndex). Spec:
    /// `_2_traits.rs`:38-40 (field) + :57-58 (ManifestMap).
    pub manifests: &'a ManifestMap,
    /// Rev-correct content reader: project-relative path -> bytes, or None.
    /// Injected by the engine; None in unit tests. Spec: `_2_traits.rs`:41-43.
    pub reader: Option<&'a dyn Fn(&str) -> Option<Vec<u8>>>,
    /// The fold of `files` + `manifests` that invalidates phase-2 on change; the
    /// middle component of the phase-2 cache key (see `Resolve`). Spec:
    /// `_2_traits.rs`:44-45.
    pub digest: ProjectDigest,
    /// Lazy, per-language whole-project indexes (v5: RustCrates, ts_packages,
    /// GoIndex). Opaque here; each language module owns its concrete index type
    /// behind a OnceLock. Spec: `_2_traits.rs`:46-51 (field) + :59-61 (IndexBag).
    pub indexes: IndexBag,
}

/// The file set: project-relative paths that exist at this rev. Hollow in 4a
/// (spec `_2_traits.rs`:56); the concrete set lands with the first Resolve impl.
pub struct FileSet;
/// Manifest path -> raw manifest contents. Hollow in 4a (spec `_2_traits.rs`:58).
pub struct ManifestMap;
/// The whole-project index bag (spec `_2_traits.rs`:59-61). ADDENDUM 4a
/// (design-audit must-encode): TWO kinds of slot, kept distinct at the type
/// level so 4b-4d cannot grow three lang-specific name indexes —
/// - `def_index`: THE corpus name index. ONE lang-agnostic slot, built ONCE per
///   refresh by `build_def_index` from ALL files' phase-1 ExtractOutputs (CallF
///   defs + TypeF entities) — never per-lang, never by re-parsing
///   `ProjectCx.reader` bytes.
/// - per-language erased slots (RustCrates / ts_packages / GoIndex): the seed's
///   per-lang OnceLock shape (`_2_traits.rs`:46-51) covers THIS kind only; they
///   land in 4b+ behind the same OnceLock discipline.
/// - `scip_index` (commit 4c): THE Tier-1 resolution index, one corpus-wide
///   slot, set once per refresh by the engine/ratchet when an index.scip is in
///   hand. `Resolve<CallF>` reads it for the ScipOverride leg; an unset slot
///   means pure name-match resolution (no scip ground truth loaded).
#[derive(Default)]
pub struct IndexBag {
    pub def_index: std::sync::OnceLock<DefIndex>,
    pub scip_index: std::sync::OnceLock<ScipIndex>,
}

// ── the corpus name index + shared resolve helpers (ADDENDUM 4a) ────────────
// Declared so the three lang resolve arms (4b TS, 4c SCIP/call, 4d rust/go)
// share ONE index + ONE set of pure helpers instead of copy-pasting resolution
// logic. Implemented in 4b-i (pure, zero AST); call sites land with the resolve
// arms themselves.

/// One definition site in the corpus: the blob + span of a def node, plus the
/// erased `family` leg of its node identity `(family, span, kind)`. The family
/// leg is REQUIRED: a class constructor is BOTH a TypeF entity and a CallF def
/// at the SAME span (the two facets join on one coordinate), so (blob, span)
/// alone does not name one node.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct DefSite {
    pub blob: BlobHash,
    pub span: Span,
    pub family: FamilyTag,
}

/// THE corpus name index: def name -> every def site with that name, across
/// ALL files and BOTH def-bearing families (CallF defs + TypeF entities).
/// Lang-agnostic by construction: it is built from phase-1 OUTPUT (the
/// ExtractOutputs, interned strings included — the NameId -> &str lookup lives
/// on `ExtractOutput.strings`), so it never re-parses and never special-cases
/// a language. Keys are owned Strings so the index outlives any one file's
/// interner.
#[derive(Clone, Debug, Default)]
pub struct DefIndex {
    pub map: std::collections::HashMap<String, Vec<DefSite>>,
}

/// Build THE `DefIndex` ONCE per refresh, from every file's phase-1
/// `ExtractOutput` paired with its blob hash. Never per-lang, NEVER from
/// `ProjectCx.reader` bytes (re-parsing in phase 2 is the triplication the
/// phase split exists to prevent). Reaches `Resolve::resolve` through
/// `ProjectCx.indexes.def_index` — NOT an explicit param: whole-project state
/// built once per refresh is exactly what the cx exists to carry, and a param
/// would invite per-call rebuilds beside the cx.
pub fn build_def_index(outputs: &[(BlobHash, &ExtractOutput)]) -> DefIndex {
    let mut index = DefIndex::default();
    for (blob, output) in outputs {
        if let Some(call) = &output.call {
            for node in &call.nodes {
                if let Some(name) = node.name {
                    index
                        .map
                        .entry(output.strings.lookup(name).to_string())
                        .or_default()
                        .push(DefSite {
                            blob: *blob,
                            span: node.span,
                            family: CallF::TAG,
                        });
                }
            }
        }
        if let Some(types) = &output.types {
            for node in &types.nodes {
                if let Some(name) = node.name {
                    index
                        .map
                        .entry(output.strings.lookup(name).to_string())
                        .or_default()
                        .push(DefSite {
                            blob: *blob,
                            span: node.span,
                            family: TypeF::TAG,
                        });
                }
            }
        }
    }
    index
}

/// Caller binding: the CallF def node whose span most tightly CONTAINS `site`
/// (the innermost covering def), by binary search over def spans sorted by
/// (start, end). Written ONCE here, used by all three lang resolve arms —
/// every lang emits body-covering def spans by design, so one sorted-span
/// search serves ts, rust, and go uniformly. Pure fn over the bundle; zero AST.
pub fn covering_def(defs: &FamilyBundle<CallF>, site: Span) -> Option<NodeRef> {
    // Sort (span, ref) by (start, end); every container of `site` starts at or
    // before it, so binary-search that cut and scan the prefix for the tightest
    // cover. Def spans nest properly (a body-covering def never partially
    // overlaps another), so min span length IS the innermost def.
    let mut sorted: Vec<(Span, NodeRef)> = defs
        .nodes
        .iter()
        .enumerate()
        .map(|(ix, node)| (node.span, NodeRef(ix as u32)))
        .collect();
    sorted.sort_by_key(|(span, _)| (span.start, span.end()));
    let cut = sorted.partition_point(|(span, _)| span.start <= site.start);
    let mut best: Option<(Span, NodeRef)> = None;
    for &(span, r) in &sorted[..cut] {
        if site.end() <= span.end()
            && best.map_or(true, |(b, _)| span.end() - span.start < b.end() - b.start)
        {
            best = Some((span, r));
        }
    }
    best.map(|(_, r)| r)
}

/// Same-file name lookup: the CallF def node in `defs` whose interned name is
/// `name` (the same-file fast path before the corpus `DefIndex` join). Written
/// ONCE here, used by all three lang resolve arms. Pure fn over the bundle +
/// its interner; zero AST.
pub fn def_named(defs: &FamilyBundle<CallF>, strings: &Strings, name: &str) -> Option<NodeRef> {
    defs.nodes
        .iter()
        .position(|node| node.name.map_or(false, |id| strings.lookup(id) == name))
        .map(|ix| NodeRef(ix as u32))
}

/// Cross-file name lookup: every def site in the corpus named `name` (the
/// `DefIndex` join behind Resolve<CallF>'s NameResolve). Written ONCE here,
/// used by all three lang resolve arms. Pure fn over the index; zero AST.
pub fn corpus_defs<'a>(index: &'a DefIndex, name: &str) -> &'a [DefSite] {
    index.map.get(name).map(Vec::as_slice).unwrap_or(&[])
}

/// Which blob produced `output`: the 4b-iii self-blob trick generalized. A
/// named def node in THIS output (CallF def or TypeF entity) joined against
/// its own `DefSite` gives the file's content key — the arm learns its own
/// blob with NO path and NO bytes (the resolve seam carries neither). None
/// when the output has no named def (a file with nothing to resolve FROM) or
/// the output was not in the index's corpus.
pub fn own_blob(output: &ExtractOutput, index: &DefIndex) -> Option<BlobHash> {
    let mut named_spans: Vec<Span> = Vec::new();
    if let Some(call) = &output.call {
        named_spans.extend(
            call.nodes
                .iter()
                .filter(|n| n.name.is_some())
                .map(|n| n.span),
        );
    }
    if let Some(types) = &output.types {
        named_spans.extend(
            types
                .nodes
                .iter()
                .filter(|n| n.name.is_some())
                .map(|n| n.span),
        );
    }
    named_spans.sort();
    named_spans.dedup();
    for span in named_spans {
        if let Some(site) = index.map.values().flatten().find(|site| site.span == span) {
            return Some(site.blob);
        }
    }
    None
}

/// The corpus def site in `blob` whose node span CONTAINS `span` (the scip
/// def-occurrence join: scip's def range marks the def IDENTIFIER, which sits
/// inside v6's whole-declaration def span). Prefers the CallF facet (a call
/// edge binds a callable), then the smallest containing span (the innermost
/// def). Returns the index's map-key name + the site. Pure fn over the index;
/// zero AST. Written ONCE here for the ts/rust/go resolve arms (4c/4d).
pub fn containing_def_site(
    index: &DefIndex,
    blob: BlobHash,
    span: Span,
) -> Option<(&str, DefSite)> {
    let mut best: Option<(&str, DefSite)> = None;
    for (name, sites) in &index.map {
        for site in sites {
            if site.blob != blob
                || !(site.span.start <= span.start && span.end() <= site.span.end())
            {
                continue;
            }
            let better = match best {
                None => true,
                Some((_, b)) => {
                    let call_bias = (site.family == CallF::TAG, b.family == CallF::TAG);
                    call_bias.0 && !call_bias.1
                        || (call_bias.0 == call_bias.1
                            && site.span.end() - site.span.start < b.span.end() - b.span.start)
                }
            };
            if better {
                best = Some((name.as_str(), *site));
            }
        }
    }
    best
}

/// Phase 2: "here is a codebase, get data" — the cross-file resolution half of
/// a `Source` (spec: seed `_2_traits.rs`:80-97 `ProjectExtract`, adapted to the
/// crate's type-level families + Epic U's uniform surface).
///
/// CACHE KEY (vs phase 1): phase 2 keys on `(BlobHash, ProjectDigest,
/// FamilyMask)` where phase 1 keys on `(BlobHash, lang, FamilyMask)`. Identical
/// BYTES anywhere extract once, but a file appearing/disappearing can change a
/// resolution, so the project digest rides the phase-2 key (spec:
/// `_2_traits.rs`:9-15,80-84; `_7_tasks.rs`:37-38).
///
/// WHICH FAMILIES RESOLVE (spec `_2_traits.rs`:80-84): `TypeF` (field / impl /
/// variant / generic / uses + the resolved param/returns binding) and `CallF`
/// (resolved caller -> callee). MODULE resolves only WHEN it lands: ModuleF is
/// still commented out (S2 above, "PENDING - collapsed"), so 4a declares NO
/// ModuleF resolve surface — this note is the module placeholder. `DfF` and
/// `CstF` NEVER resolve (no cross-file resolution; `_2_traits.rs`:82-84).
///
/// SHAPE NOTES (4a judgment calls, flagged for human review):
/// - `output` is the whole phase-1 `ExtractOutput`, not a bare
///   `FamilyBundle<F>`: resolution joins on NAMES, and the interner that turns
///   a `NameId` back into a &str lives on `ExtractOutput.strings`. (Arc plan
///   2026-07-23:503-504: "resolve(&ExtractOutput, &ProjectCx)".)
/// - No `FamilyMask` param: unlike the seed's non-generic `ProjectExtract`,
///   `F` is a type parameter here, so the family is already selected per impl.
/// - `ProjectEdge<F>` is generic because the seed's `EdgeKind` sum is deleted
///   (D-families); the plan's `Vec<ProjectEdge>` reads `Vec<ProjectEdge<F>>`.
/// - ADDENDUM 4a: the corpus `DefIndex` arrives through `cx.indexes.def_index`,
///   not an explicit param — whole-project state built once per refresh is
///   exactly what the cx exists to carry (see `IndexBag` / `build_def_index`).
pub trait Resolve<F: Family>: Source {
    /// Turn this file's phase-1 specifiers/names into resolved, cross-file
    /// `ProjectEdge`s. The return is ONLY the cross-file resolutions for this
    /// one blob (spec: `_2_traits.rs`:88-96).
    fn resolve(&self, output: &ExtractOutput, cx: &ProjectCx) -> Vec<ProjectEdge<F>> {
        let _ = (output, cx);
        todo!("4b-iii landed Resolve<TypeF> + 4c-ii Resolve<CallF> for TsSource; 4d landed both arms for RustSource; next: 4d go arms")
    }
}

// ── S6 SCIP: the Tier-1 resolution wire (commit 4c) ─────────────────────────
// The diet slice of scip.proto (seed `_4_scip.rs`): "we keep ONLY the fields
// that project onto the four families (symbol + range + role + ...), and drop
// the rest". Boundary law (seed, owner ruling): indexers are FOREIGN TOOLS
// behind a subprocess seam — NEVER a bespoke indexer, NEVER compiler FFI. The
// concrete impl (`ScipTypescript`: build subprocess + load protobuf decode)
// lives in `crate::scip`; only types + the seam trait live here.

/// The SCIP `SymbolRole` bitfield, lifted from scip.proto (seed
/// `_4_scip.rs`:46-61). Composes (a definition that is also a write).
#[derive(Copy, Clone, Debug, PartialEq, Eq, Default)]
pub struct OccurrenceRole(pub i32);

impl OccurrenceRole {
    pub const DEFINITION: Self = Self(0x1);
    pub const IMPORT: Self = Self(0x2);
    pub const WRITE_ACCESS: Self = Self(0x4);
    pub const READ_ACCESS: Self = Self(0x8);
    pub const GENERATED: Self = Self(0x10);
    pub const TEST: Self = Self(0x20);
    pub const FORWARD_DEF: Self = Self(0x40);
    pub fn contains(self, bit: Self) -> bool {
        (self.0 & bit.0) != 0
    }
}

/// SCIP `PositionEncoding` (scip.proto): the column unit of every occurrence
/// range in one document. `Unspecified` reads as UTF-16 per the SCIP spec.
#[derive(Copy, Clone, Debug, Default, PartialEq, Eq)]
pub enum PositionEncoding {
    #[default]
    Unspecified,
    Utf8,
    Utf16,
    Utf32,
}

/// One occurrence: a (symbol, range, roles) triple — a definition or a
/// reference site (seed `_4_scip.rs`:26-35). `range` is scip.proto's packed
/// quad normalized to `[start_line, start_col, end_line, end_col]` (the 3-
/// element short form expanded), 0-based, cols in the document's
/// `PositionEncoding` — deliberately NOT a v6 byte `Span`: the line/col ->
/// byte bridge needs the document's content, which only the consumer holds
/// (the ratchet / the engine). `crate::scip::byte_range` is that bridge.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct ScipOccurrence {
    pub symbol: String,
    pub range: [i32; 4],
    pub roles: OccurrenceRole,
}

/// Diet `SymbolInformation` (seed `_4_scip.rs`:37-44): the identity string +
/// its display name + the raw kind ordinal. Markdown docs, type signatures,
/// and relationships are dropped per the diet law.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct ScipSymbolInfo {
    pub symbol: String,
    pub display_name: String,
    pub kind: i32,
}

/// One indexed document: path relative to the indexed root + occurrences +
/// symbol infos (seed `_4_scip.rs`:96-104). NO blob leg: the content join
/// (relative_path -> reader/content) is the consumer's — the seed's
/// `ScipDocument.blob` is a join product, not a parse product.
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct ScipDocument {
    pub relative_path: String,
    pub position_encoding: PositionEncoding,
    pub occurrences: Vec<ScipOccurrence>,
    pub symbols: Vec<ScipSymbolInfo>,
}

/// One parsed index.scip: documents + externally-referenced symbols + the
/// producing indexer identity (metadata.tool_info, kept for the ledger).
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct ScipIndex {
    pub documents: Vec<ScipDocument>,
    pub external_symbols: Vec<ScipSymbolInfo>,
    pub tool: String,
}

/// Why a scip build/load failed (seed `_4_scip.rs`:126-132; String payloads
/// where the subprocess/decode detail matters for the report).
#[derive(Debug)]
pub enum ScipError {
    /// The foreign tool is not installed (no PATH binary, no npx fallback).
    IndexerMissing(&'static str),
    /// Non-zero exit from the indexer subprocess (stderr tail attached).
    IndexerFailed(String),
    /// The index file could not be read / protobuf decode failed.
    Parse(String),
}

impl fmt::Display for ScipError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            ScipError::IndexerMissing(name) => write!(f, "scip indexer not found: {name}"),
            ScipError::IndexerFailed(msg) => write!(f, "scip indexer failed: {msg}"),
            ScipError::Parse(msg) => write!(f, "scip index parse failed: {msg}"),
        }
    }
}

impl std::error::Error for ScipError {}

/// The Tier-1 source seam (seed `_4_scip.rs`:118-124). Two ops, both SYNC +
/// CPU-bound: `build` shells out the foreign indexer over `root`, writing
/// index.scip to a HERMETIC temp path (the source dir is never written — the
/// indexer's inferred tsconfig lands in a staged copy) and returning that
/// path; `load` parses an index.scip into the diet `ScipIndex`. The seed's
/// `build -> Result<(), _>` reads `-> Result<PathBuf, _>` here: the caller
/// must know where the hermetic output landed.
pub trait ScipSource: Sync + Send {
    fn indexer(&self) -> &'static str;
    fn build(&self, root: &std::path::Path) -> Result<std::path::PathBuf, ScipError>;
    fn load(&self, index_path: &std::path::Path) -> Result<ScipIndex, ScipError>;
}

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
    pub const ALL: Self = Self {
        cst: true,
        types: true,
        call: true,
        df: true,
    };
    pub const NONE: Self = Self {
        cst: false,
        types: false,
        call: false,
        df: false,
    };
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
    /// CallF module specifier (phase-1, as written): span, bound name, kind.
    /// v6-ONLY rows (no v5 oracle facet) — the parity golden reports them,
    /// never asserts them.
    Specifier {
        family: FamilyTag,
        span: SpanOut,
        name: String,
        kind: String,
    },
    /// A project-phase (cross-file) resolved edge: `to` lives in ANOTHER blob,
    /// content-keyed by `to_blob` (hex). The 4a wire ruling: ONE arm carries
    /// TypeEdgeKind + CallEdgeKind as strings (never per-family arms, never a
    /// side channel). Emitted only when a `Resolve<F>` result is flattened —
    /// `flatten_jsonl` (the CLI stream) stays phase-1 and never produces these.
    ProjectEdge {
        family: FamilyTag,
        kind: String,
        from: SpanOut,
        to_blob: String,
        to: SpanOut,
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
//         &GoSource,      //  .go               tree-sitter-go           (lang/go.rs)
//         &KotlinSource,  //  .kt/.kts          tree-sitter-kotlin-sg    (lang/kotlin.rs)
//         &TsSource,      //  .ts/.tsx/.js/...  oxc front-end            (lang/ts.rs)
//         &AstgrepSource, //  fallback: cst-only for any ast-grep grammar
//     ]
// }
// (KotlinSource precedes TsSource: "x.kts".ends_with(".ts") routes .kts to kotlin.)

// ════════════════════════════════════════════════════════════════════════════
// STATUS  (flip a cell when it ships; [x] = ported + parity-green)
// ════════════════════════════════════════════════════════════════════════════
//
//                          TS (oxc)   Rust (syn)   Go (tree-sitter-go)   Kotlin (ts-kotlin-sg)
//   cst (ast-grep)           [x]         [x]            [x]                 [x]
//   type entities + sigs     [x]         [x]            [x]                 [x]
//   const facet              [x]         [x]            [-] n/a (v5 go emits none)   [-] n/a (v5 kotlin emits none)
//   call defs + sites        [x]         [x]            [x]                 [x]
//   df nodes + edges         [x]         [x]            [x]                 [x]
//   parity vs v5 oracle      [x]         [x]            [x]                 [x]
//
//   Parity is asserted with ZERO waivers: the closure df-node name (v5's
//   lam_sym) is ported - minted in the df walks from span/containment data.
//
// DEFERRED (per-lang gates noted; the rest lands with Resolve<F>/follow-ups):
//   type_edge (field/impl/variant/uses/generic)   -> TS ASSERTED (4b-iii); GO ASSERTED (4d-i-go, v5 go shape-only: field/impl/generic); rust ASSERTED (4d-i-rust; no sig-sourced rows per v5); kotlin DEFERRED to the traits/codegen arc (v5 kotlin DOES emit: field/impl/generic/variant - candidates + Resolve<TypeF> land there)
//   resolved caller -> callee                     -> TS RATCHETED vs scip (4c-ii); GO RATCHETED vs scip-go (4d-ii-go); rust RATCHETED vs rust-analyzer-scip (4d-ii-rust); kotlin DEFERRED to the traits/codegen arc
//   docs facet                                    -> follow-up
//   df aux (args/fields/lits/param_pos)           -> labels, follow-up
//
// LEAF INFRA (pure CPU; still this leaf): parallel dispatch (rayon, arena-per-
//   worker); BlobSource impls + the (BlobHash, lang, mask) content-keyed cache.
//
// OUT OF SCOPE (engine, another worktree): store-seam wiring (seed Extract trait
//   is todo!()), datalog fixpoint, reactivity, async-eval.
