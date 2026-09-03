//! THE canonical type module for the sprefa-extract leaf.
//!
//! Every public type / trait / enum / struct / impl lives here. The other modules
//! (shape, family, rows, seams, source) are `pub use crate::types::*` re-export
//! shims so historical import paths (`crate::shape::Span`, `crate::family::TypeF`,
//! ...) keep resolving. This is the "tasks.rs technique" from the seed, promoted:
//! one compiled file is the source of truth, and a shape kept only as a revival
//! sketch is COMMENTED OUT (ModuleF).
//!
//! Leaf scope: a corpus at a version -> normalized graph facts. Pure CPU, no SQL,
//! no datalog, no async (the engine, another worktree).
//!
//! Planes:  RESOLUTION (SCIP-wire): CallF, TypeF
//!          VALUE-FLOW (native):   DfF, FlowF
//!          STRUCTURE (lossless):  CstF
//! ModuleF is DECIDED COLLAPSED (fork C, 2026-08-17): the module plane's output
//! is the `file_edge` / `file_unresolved` / `package_edge` record trio, not a
//! family. The sketch below stays as the shape a revival would take.
// @comment-ok: the module header is a crate-level doc block predating the rail

use std::collections::{BTreeSet, HashMap};
use std::fmt;
use std::marker::PhantomData;

use ast_grep_core::tree_sitter::LanguageExt;
use ast_grep_core::Language;
use ast_grep_language::SupportLang;
use serde::Serialize;

use crate::lang::extract_lang::ExtractLang;
use crate::move_cx::MoveCx;
use crate::rename_cx::{RenameCx, RenameRequest};

pub use soopy::ContentId;

// ════════════════════════════════════════════════════════════════════════════
// S1 ATOMS
// ════════════════════════════════════════════════════════════════════════════

/// THE one coordinate. Byte offsets into the file; line/col derived, never stored.
#[derive(Copy, Clone, Debug, Default, PartialEq, Eq, PartialOrd, Ord, Hash, Serialize)]
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

/// Hash file bytes to `ContentId::Blake3` through soopy's own constructor, so
/// the corpus and soopy's enumeration cannot disagree on one file's identity.
pub fn content_id_of(content: &[u8]) -> ContentId {
    // The span sits on the hash and never on a caller: blake3 is linear in file
    // size, and a duplicate pass added at ANY call site has to reach this count.
    let span = crate::trace::phase_span("-", crate::trace::Phase::Hash);
    let _entered = span.enter();
    let id = ContentId::blake3(content);
    crate::trace::record_phase(&span, content.len() as u64, 0, 1);
    id
}

/// The no-blob sentinel: the dst leg of an edge with no corpus target.
pub const ZERO_CONTENT_ID: ContentId = ContentId::Blake3([0u8; 32]);

/// A digest of the file set that affects resolution (which files exist + their
/// manifest membership), folded from the corpus so two identical blobs in
/// identical file-set contexts share phase-2 work. The middle component of the
/// phase-2 cache key (see `Resolve`). Spec: seed `_1_mask.rs`:78-82. Declared
/// here; NOT computed yet (lands with the phase-2 cache).
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
    Flow,
    Call,
    Type,
    Module,
    Cst,
    Cfg,
    Data,
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

impl Strings {
    /// Approximate heap footprint of the interned strings, for the cache weigher.
    /// Moves with the real size; not exact.
    pub fn heap_bytes(&self) -> usize {
        self.map.len() * size_of::<(String, NameId)>()
            + self
                .names
                .iter()
                .map(|name| name.capacity() + size_of::<String>())
                .sum::<usize>()
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
/// is the family's side-channel payload (TypeF sigs/consts, CallF sites, DfF
/// parameter and argument rows): per-
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

/// One language's own kind, carried by the `Ext` variant of a kind enum. The
/// tag is the language's own snake_case string; it must never equal a core tag
/// of that enum (railed in tests/6_kind_vocab.rs), so `as_str` stays injective
/// and the wire keeps one vocabulary.
#[derive(Copy, Clone, Debug, PartialEq, Eq, Hash)]
pub struct LangKind {
    pub lang: &'static str,
    pub tag: &'static str,
}

/// type_entity kind. Core = every variant at least two languages construct
/// today; a kind one language owns lives in that language's file as an
/// `Ext(LangKind)` constant (rust.rs `TRAIT`).
#[derive(Copy, Clone, Debug, PartialEq, Eq)]
pub enum TypeEntityKind {
    Struct,
    Enum,
    Class,
    Interface,
    Alias,
    Function,
    Method,
    Const,
    Ext(LangKind),
}

impl TypeEntityKind {
    pub const fn as_str(self) -> &'static str {
        match self {
            TypeEntityKind::Struct => "struct",
            TypeEntityKind::Enum => "enum",
            TypeEntityKind::Class => "class",
            TypeEntityKind::Interface => "interface",
            TypeEntityKind::Alias => "alias",
            TypeEntityKind::Function => "function",
            TypeEntityKind::Method => "method",
            TypeEntityKind::Const => "const",
            TypeEntityKind::Ext(ext) => ext.tag,
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
    DocRef,
}

impl TypeEdgeKind {
    pub const fn as_str(self) -> &'static str {
        match self {
            TypeEdgeKind::Field => "field",
            TypeEdgeKind::Variant => "variant",
            TypeEdgeKind::Impl => "impl",
            TypeEdgeKind::Generic => "generic",
            TypeEdgeKind::Param => "param",
            TypeEdgeKind::Returns => "returns",
            TypeEdgeKind::Uses => "uses",
            TypeEdgeKind::DocRef => "doc_ref",
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
    pub const fn as_str(self) -> &'static str {
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
    pub const fn as_str(self) -> &'static str {
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

/// A doc block bound to a declared entity, keyed by the entity node's span.
/// `parent` is a method's impl owner: TypeF method nodes are bare-named.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct DocFact {
    pub owner: Span,
    pub parent: Option<NameId>,
    pub text: NameId,
    pub tags: Vec<DocTag>,
}

/// One structured doc tag. `tag` is the bare tag word (`section` for a rustdoc
/// `# Heading`); `arg` is the name the tag carries, None when it takes none.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct DocTag {
    pub tag: NameId,
    pub arg: Option<NameId>,
    pub text: NameId,
}

/// A heading, code block, link or image; `name` = title, fence language, link
/// text or image description; `target`/`title` for links, `body` for fences.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct DocNode {
    pub span: Span,
    pub kind: DocNodeKind,
    pub name: NameId,
    pub parent: Option<NameId>,
    pub target: Option<NameId>,
    pub title: Option<NameId>,
    pub body: Option<Span>,
}

/// The kind of a document structure node.
#[derive(Copy, Clone, Debug, PartialEq, Eq)]
pub enum DocNodeKind {
    Heading,
    CodeBlock,
    Link,
    Image,
}

impl DocNodeKind {
    pub const fn as_str(self) -> &'static str {
        match self {
            DocNodeKind::Heading => "heading",
            DocNodeKind::CodeBlock => "code_block",
            DocNodeKind::Link => "link",
            DocNodeKind::Image => "image",
        }
    }
}

/// The TypeF side-channel: arrow-type sigs + the const facet + the unresolved
/// type-edge candidates (4b-iii) + the doc facet + the doc structure rows + the
/// syntax tier's TSI rows.
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct TypeFAux {
    pub sigs: Vec<TypeSig>,
    pub consts: Vec<ConstValue>,
    pub candidates: Vec<TypeEdgeCandidate>,
    pub docs: Vec<DocFact>,
    pub doc_nodes: Vec<DocNode>,
    pub impl_owners: Vec<ImplOwner>,
    /// Reaches the wire only under `--witness`; the span arguments carry an
    /// empty digest until the flatten stamps the run's.
    pub tsi: Vec<crate::tsi::FactOut>,
}

/// An impl's owner when its self type is declared in ANOTHER file. Never a
/// node: `build_def_index` indexes every named node and lookups would go ambiguous.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct ImplOwner {
    pub span: Span,
    pub name: NameId,
}

/// One id per distinct WRITTEN type text per file; a member callable and a
/// generic parameter take a fresh id (identity rules 1 and 4).
pub struct TsiNames {
    sink: crate::tsi::TsiSink,
    seen: std::collections::HashMap<NameId, u32>,
    lang: &'static str,
}

impl TsiNames {
    /// `lang` is the atom `tsi.origin` carries: `ts` or `rust`.
    pub fn new(lang: &'static str) -> Self {
        Self {
            sink: crate::tsi::TsiSink::new(0, crate::tsi::Method::Parse),
            seen: std::collections::HashMap::new(),
            lang,
        }
    }

    /// The id for a written type text, minted on first sight.
    pub fn named(&mut self, strings: &mut Strings, text: &str, span: Span) -> u32 {
        let key = strings.intern(text);
        if let Some(&id) = self.seen.get(&key) {
            return id;
        }
        let id = self.anonymous(span);
        self.seen.insert(key, id);
        self.name(id, text);
        id
    }

    /// `tsi.name` for an id that already exists: the spelling a consumer prints.
    pub fn name(&mut self, id: u32, text: &str) {
        self.sink.fact(
            "tsi.name",
            vec![
                crate::tsi::Arg::Id(id),
                crate::tsi::Arg::Text(text.to_string()),
            ],
        );
    }

    /// A fresh id with an origin and no name-table entry.
    pub fn anonymous(&mut self, span: Span) -> u32 {
        let id = self.sink.fresh_id();
        self.sink.fact("tsi.type", vec![crate::tsi::Arg::Id(id)]);
        self.sink.fact(
            "tsi.origin",
            vec![
                crate::tsi::Arg::Id(id),
                crate::tsi::Arg::Atom(self.lang.to_string()),
                span_arg(span),
            ],
        );
        id
    }

    /// An id with no `tsi.type` row of its own: an edge id, or a `tsi.called`
    /// argument list. Both are declaring positions on the wire.
    pub fn bare_id(&mut self) -> u32 {
        self.sink.fresh_id()
    }

    pub fn fact(&mut self, relation: &'static str, args: Vec<crate::tsi::Arg>) {
        self.sink.fact(relation, args);
    }

    /// `tsi.edge(Edge, Owner, Label, Target, Position)`, handing back the edge
    /// id so `ts.optional` and `ts.readonly` can name it.
    pub fn edge(&mut self, owner: u32, label: &str, target: u32, position: i64) -> u32 {
        let edge = self.bare_id();
        self.fact(
            "tsi.edge",
            vec![
                crate::tsi::Arg::Id(edge),
                crate::tsi::Arg::Id(owner),
                crate::tsi::Arg::Text(label.to_string()),
                crate::tsi::Arg::Id(target),
                crate::tsi::Arg::Int(position),
            ],
        );
        edge
    }

    /// `tsi.origin` for a declaration whose id already exists.
    pub fn origin(&mut self, id: u32, span: Span) {
        self.sink.fact(
            "tsi.origin",
            vec![
                crate::tsi::Arg::Id(id),
                crate::tsi::Arg::Atom(self.lang.to_string()),
                span_arg(span),
            ],
        );
    }

    /// The sink's witness and coverage rows are dropped: the wire mints its own
    /// over the whole stream, so a second copy would double every witness.
    pub fn into_facts(self) -> Vec<crate::tsi::FactOut> {
        self.sink
            .rows()
            .into_iter()
            .filter_map(|row| match row {
                FlatFact::Fact(fact) => Some(fact),
                _ => None,
            })
            .collect()
    }
}

/// The digest is empty here and stamped by the flatten, the one layer that
/// holds the file's bytes.
pub fn span_arg(span: Span) -> crate::tsi::Arg {
    crate::tsi::Arg::Span(String::new(), span.start, span.end())
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

/// The call-def node shape. `Free` wires as "function" (v5 parity). Every
/// variant is constructed by four or more languages today; `Ext` is the door a
/// language uses when it needs a kind the core lacks.
#[derive(Copy, Clone, Debug, PartialEq, Eq)]
pub enum CallKind {
    /// A free function. Wire tag is "function" (v5 CallKind::Free.tag()).
    Free,
    /// A class method (incl. the constructor).
    Method,
    /// An anonymous callable from the df lift (emitted by the DfF pass).
    Lambda,
    Ext(LangKind),
}

impl CallKind {
    pub const fn as_str(self) -> &'static str {
        match self {
            CallKind::Free => "function",
            CallKind::Method => "method",
            CallKind::Lambda => "lambda",
            CallKind::Ext(ext) => ext.tag,
        }
    }
}

/// How a resolved call edge's callee was bound. Emitted by Resolve<CallF>.
/// `Implements` is additive: only go emits it (interface spec -> implementer).
#[derive(Copy, Clone, Debug, PartialEq, Eq)]
pub enum CallEdgeKind {
    /// The callee name resolved to exactly one def in the corpus.
    NameResolve,
    /// SCIP overrode the AST name-match.
    ScipOverride,
    /// A callable NAMED as a value, never called at that spot: the edge a
    /// `position=value` reference row resolves to.
    ValueRef,
    /// The callee is an import binding, bound through the language's own module
    /// plane (ResolveExport) rather than by name-matching across the corpus.
    ImportResolve,
    /// An interface method spec bound to one implementing type's method.
    Implements,
    /// The call is written inside a macro invocation: the parse walk never saw
    /// a site, so the scip occurrence at the expanded position bound the edge.
    /// Minted by the project post-pass, never by a per-file `Resolve` arm.
    ScipMacro,
    /// The language's own checker named the destination; the syntax leg's
    /// answer, where it had one, was overridden.
    CheckerResolve,
}

impl CallEdgeKind {
    pub const fn as_str(self) -> &'static str {
        match self {
            CallEdgeKind::NameResolve => "name_resolve",
            CallEdgeKind::ScipOverride => "scip_override",
            CallEdgeKind::ValueRef => "value_ref",
            CallEdgeKind::ImportResolve => "import_resolve",
            CallEdgeKind::Implements => "implements",
            CallEdgeKind::ScipMacro => "scip_macro",
            CallEdgeKind::CheckerResolve => "checker_resolve",
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

/// A `Method` def's declaration, keyed by the def node's span. Two seats, never
/// one: `impl Draw for A` and `impl Erase for A` differ only in `trait_name`.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct MethodOwner {
    pub span: Span,
    /// The impl's primary self type; `None` for a trait declaration's own items.
    pub self_type: Option<NameId>,
    /// The implemented or declaring trait; `None` for an inherent impl.
    pub trait_name: Option<NameId>,
}

/// A call site's receiver-type outcome, keyed by `CallSite.span` (`call_site`).
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct ReceiverBinding {
    pub call_site: Span,
    pub outcome: ReceiverOutcome,
}

/// How a call site's receiver expression's static type was determined.
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum ReceiverOutcome {
    /// The declared type name (var/param/field/receiver, pointer/slice/map
    /// unwrapped to the element or value type).
    Named(NameId),
    /// A `:=` bound to a call result: out of scope by policy.
    Inferred,
    /// Two conflicting type declarations bind the same name in this scope.
    Ambiguous,
}

/// A Prolog term-occurrence reference: a compound constructed or destructured in
/// argument position. The only family that emits these is the Prolog front-end;
/// the field rides the shared `CallFAux` so the wire needs no per-lang slot.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Reference {
    pub span: Span,
    /// The interned `functor/arity` key, arity 0 for a bare name used as a goal.
    pub functor: NameId,
    pub position: RefPosition,
}

/// Where a Prolog compound sits: executed as a goal, inside a clause head's
/// arguments, inside another term's arguments (data), or in a meta-predicate
/// closure slot (`maplist(double, ...)`) where the callee gains extra args.
#[derive(Copy, Clone, Debug, PartialEq, Eq)]
pub enum RefPosition {
    Goal,
    HeadArg,
    TermArg,
    Closure,
    /// A callable NAMED as a value, never called at that spot
    /// (`transformers.push(transformES2015)`). Minted by the ts arm.
    Value,
}

impl RefPosition {
    pub const fn as_str(self) -> &'static str {
        match self {
            RefPosition::Goal => "goal",
            RefPosition::HeadArg => "head_arg",
            RefPosition::TermArg => "term_arg",
            RefPosition::Closure => "closure",
            RefPosition::Value => "value",
        }
    }
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
/// consumes specifiers before Resolve<CallF>), so it stayed unadded.
///
/// THE FROM-MODULE GAP IS CLOSED as of the diet-resolution lane: `module` is
/// the source module text as written (`./x.ts`, `rxjs`, `node:fs`), and it is
/// what `crate::deps` resolves to a file path. The gap was real, not stylistic
/// — with a bound name and no module, a specifier row states that something
/// entered scope and refuses to say from where, which makes the module graph
/// inexpressible from phase-1 rows. `None` is for the languages that emit
/// specifiers with the module already in `name` (go's path-only imports).
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Specifier {
    pub span: Span,
    pub name: NameId,
    pub kind: SpecifierKind,
    pub module: Option<NameId>,
    /// The name the SOURCE module spells, when the local binding renames it
    /// (`import {inner as local}`, a default import's `default`). `None` when
    /// local and imported agree, or when `module`'s trailing segment already
    /// spells it (the path-shaped languages: rust, go, kotlin, dl6, prolog).
    // @comment-ok: v5's module_binding carried (local, imported, kind); this is the imported seat
    pub imported: Option<NameId>,
}

/// An edge whose target is computed at runtime. `span` is the computed
/// expression itself, so `detail` is exactly the source text at `span`.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Unresolved {
    pub span: Span,
    pub reason: UnresolvedReason,
    pub detail: NameId,
}

/// The closed v5 vocabulary (`src/engine/family/mod.rs:552-570`) plus two
/// resolve-phase reasons (`issues/extract-unresolved-resolve-phase-reasons`).
/// `Builtin`/`Inferred` are additive; every existing arm keeps emitting only
/// its original reasons.
#[derive(Copy, Clone, Debug, PartialEq, Eq)]
pub enum UnresolvedReason {
    DynamicImport,
    ComputedMemberCall,
    SpreadCallArgs,
    /// No corpus def bears the callee's name: std or a dependency.
    NoCorpusDef,
    /// The corpus defines the name and this tier cannot say which one is meant.
    Ambiguous,
    /// A predeclared identifier (builtin func or conversion), not a corpus gap.
    Builtin,
    /// A receiver type this tier declines to trace (a `:=` bound to a call
    /// result), not a missing declaration.
    Inferred,
    /// An import spec whose target directory carries no corpus file: outside
    /// the declaring module, or simply not part of this run's file set.
    External,
    /// An interface dispatch site whose interface has more than 64
    /// implementers: the `I.M` spec edge stays, the fan-out is capped.
    FanoutCap,
}

impl UnresolvedReason {
    pub const fn as_str(self) -> &'static str {
        match self {
            UnresolvedReason::DynamicImport => "dynamic-import",
            UnresolvedReason::ComputedMemberCall => "computed-member-call",
            UnresolvedReason::SpreadCallArgs => "spread-call-args",
            UnresolvedReason::NoCorpusDef => "no_corpus_def",
            UnresolvedReason::Ambiguous => "ambiguous",
            UnresolvedReason::Builtin => "builtin",
            UnresolvedReason::Inferred => "inferred",
            UnresolvedReason::External => "external",
            UnresolvedReason::FanoutCap => "fanout_cap",
        }
    }
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
    Include,
    ReexportModule,
    /// `import('./m')`. Nothing enters scope by name, so `name` is the module
    /// path, the same seat the path-only forms use.
    DynamicImport,
    /// `require('./m')` and `import x = require('./m')`. `name` is the bound
    /// name when the form has one, else the module path.
    Require,
}

impl SpecifierKind {
    pub const fn as_str(self) -> &'static str {
        match self {
            SpecifierKind::Named => "named",
            SpecifierKind::Default => "default",
            SpecifierKind::Namespace => "namespace",
            SpecifierKind::SideEffect => "side_effect",
            SpecifierKind::Reexport => "reexport",
            SpecifierKind::Include => "include",
            SpecifierKind::ReexportModule => "reexport_module",
            SpecifierKind::DynamicImport => "dynamic_import",
            SpecifierKind::Require => "require",
        }
    }
}

/// The CallF side-channel: call sites + module specifiers (both phase-1
/// unresolved). Specifier rows
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
    /// Runtime-computed edge markers (dynamic import / computed member call /
    /// spread call args). Port of v5 `UnresolvedRef`.
    pub unresolved: Vec<Unresolved>,
    /// Prolog term-occurrence references. Only the Prolog front-end populates
    /// this; other languages leave it empty.
    pub refs: Vec<Reference>,
    /// One row per `Method` def whose declaration names an owner, joined to the
    /// def node by span. Rust populates it; other languages leave it empty.
    pub method_owners: Vec<MethodOwner>,
    /// One row per def that a `#[cfg(...)]` predicate naming `test` guards,
    /// its own or an enclosing module's, joined to the def node by span.
    /// Emitted ONLY for guarded defs, so a consumer declaring the `cfg` column
    /// receives exactly the conditional set and nothing else.
    pub cfg_scopes: Vec<CfgScope>,
    /// One row per callee this file names ONLY from cfg-guarded sites, so a
    /// consumer can subtract the name and still keep every shipped call.
    pub test_only_calls: Vec<TestOnlyCall>,
    /// One row per call site whose receiver type this file could trace, joined
    /// to `CallSite.span`. Go populates it; other languages leave it empty.
    pub receivers: Vec<ReceiverBinding>,
    /// One row per macro invocation that minted a def/site elsewhere in this
    /// bundle, joined by span to whatever phase-1 arm found the expansion.
    pub macro_sites: Vec<MacroSite>,
    /// Python-only dynamic-shape rows (one seat on the shared aux, exactly like
    /// `refs` for prolog; every other language leaves these empty). Collected
    /// by the python `project_call`, consumed by `Resolve<CallF>` for the call
    /// shapes a bare callee name cannot carry: same-file value bindings, call
    /// arguments, params, single returns, subscript/return-call sites.
    pub py_binds: Vec<PyBind>,
    pub py_args: Vec<PyCallArg>,
    pub py_params: Vec<PyParam>,
    pub py_defaults: Vec<PyDefault>,
    pub py_returns: Vec<PyReturn>,
    pub py_sub_calls: Vec<PySubCall>,
    pub py_ret_calls: Vec<PyRetCall>,
    /// `target = <call>(...)`: the name is bound to whatever the call's def
    /// returns. A matching `PyBind` KILL row sits at the same span, so the
    /// byte-order lookup treats the two as one rebinding.
    pub py_call_binds: Vec<PyCallBind>,
    /// One row per decorated def, from the OUTERMOST decorator only: the
    /// decorator call site (`span`), its callee, and the decorated def name.
    /// A decorator whose def's single return names a same-file def rebinds the
    /// decorated name to it.
    pub py_decorators: Vec<PyDecor>,
}

/// A same-file value binding: `target = <value name>` (simple alias, chained
/// assignment, tuple/starred unpack element; the value name is a bare
/// identifier, a trailing attribute name, or a lambda's `<lambdaN>` def
/// name), or a container element `target[key] = value` / a literal pair / a
/// list slot. `key` is the literal key path: unquoted string content, or
/// `#` + decimal for an integer / list slot, nested levels joined by `\x1f`
/// (`d["a"][0]` is `a\x1f#0`); None for a plain name binding. Emitted in file
/// order. `value` None marks a KILL: the target was rebound to something this
/// tier does not carry, so an earlier binding must not survive it.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct PyBind {
    /// The binding's own span (the assignment or the literal element), the
    /// byte-order key a lookup uses.
    pub span: Span,
    /// The bound name (LHS identifier; for `Elem`, the container's base name).
    pub target: NameId,
    /// Literal dict key / list index, when the binding is a container element.
    pub key: Option<NameId>,
    /// The value as written (a bare identifier), or None (a non-name kill).
    pub value: Option<NameId>,
}

/// One call argument that names a value (a bare identifier, a trailing
/// attribute name, or a lambda's `<lambdaN>` def name): `f(g)` / `f(x=g)`,
/// keyed by the call site's span. The param rule reads these; a decorator
/// application emits one too (the decorated def is the decorator's slot-0
/// argument).
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct PyCallArg {
    /// The owning call site's span (the function-node span).
    pub site: Span,
    /// 0-based positional slot; a keyword argument keeps its position too.
    pub pos: i64,
    /// The keyword name, when the argument is spelled `name=value`.
    pub kw: Option<NameId>,
    /// The argument's identifier text.
    pub value: NameId,
}

/// A parameter `name` (slot `pos`) of the def spanning `def`.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct PyParam {
    pub def: Span,
    pub name: NameId,
    pub pos: u32,
}

/// A parameter default that is a bare identifier: `def f(a=func)`.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct PyDefault {
    pub def: Span,
    pub name: NameId,
    pub value: NameId,
}

/// The def spanning `def` has exactly ONE return statement and its value is a
/// bare identifier. Resolution checks the value against the corpus; a value
/// naming no def (a param, say) simply resolves to nothing there.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct PyReturn {
    pub def: Span,
    pub value: NameId,
}

/// A call whose callee is `base[key]` with a literal key: the `CallSite` with
/// the same span resolves through `PyBind`'s Elem rows, never through the bare
/// base name.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct PySubCall {
    /// The subscript node's span (the CallSite span for this site).
    pub span: Span,
    pub base: NameId,
    pub key: NameId,
}

/// `target = f(...)`: `target` is bound to what the def behind the call site
/// at `site` (its function-node span) returns, through that def's single
/// return.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct PyCallBind {
    pub span: Span,
    pub target: NameId,
    pub site: Span,
}

/// A call whose function is itself a call (`f()(...)`): the `CallSite` with
/// span `span` has no name; its callee is whatever the inner call (whose site
/// has span `inner`) returns, traced through the def's single return.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct PyRetCall {
    /// The outer site's span (the inner call node's extent).
    pub span: Span,
    /// The inner call site's span (the inner call's function-node span).
    pub inner: Span,
}

/// One decorated def, from its outermost decorator: the decorator expression's
/// span (a `CallSite` with the same span carries the edge), the callee, and
/// the decorated def's name.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct PyDecor {
    pub span: Span,
    pub callee: NameId,
    pub decorated: NameId,
    /// The decorator expression was itself a call (`@factory()`): the def it
    /// resolves to may return the APPLIED decorator, and that application is
    /// its own call edge.
    pub call_expr: bool,
}

/// One macro invocation whose expansion is folded into this bundle's own
/// nodes/sites. `span` is the invocation's span, never the expansion's.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct MacroSite {
    pub span: Span,
    pub macro_name: NameId,
    pub source: MacroSiteSource,
}

/// Which arm minted the expansion this `MacroSite` reports.
#[derive(Copy, Clone, Debug, PartialEq, Eq)]
pub enum MacroSiteSource {
    /// In-process `macro_rules!` expansion (`rust_mbe::expand_file`).
    Mbe,
    /// A scip occurrence inside a macro invocation span, joined post-resolve.
    Scip,
}

impl MacroSiteSource {
    pub const fn as_str(self) -> &'static str {
        match self {
            MacroSiteSource::Mbe => "mbe",
            MacroSiteSource::Scip => "scip",
        }
    }
}

/// A def the compiler only builds under a cfg predicate. Carried so a caller
/// counting definitions can subtract the ones that never reach a release
/// binary, which otherwise inflate every per-file count.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct CfgScope {
    pub span: Span,
    pub cfg: NameId,
}

/// A callee EVERY site in this file names under a cfg predicate naming `test`.
/// One site outside the predicate keeps the callee off this list.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct TestOnlyCall {
    pub callee: NameId,
    pub cfg: NameId,
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

/// One parameter-to-slot bridge for the df wire. `node` points at a `param`
/// node in the same DfF bundle; `pos` counts typed parameters, omitting Rust
/// receivers and matching the v5 `df_param` contract.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct DfParam {
    pub node: NodeRef,
    pub pos: u32,
}

/// One call/new argument-to-slot bridge for the df wire. `pos` is signed so a
/// method receiver occupies slot `-1`; ordinary and named arguments retain
/// their source slot.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct DfArg {
    pub call: NodeRef,
    pub pos: i64,
    pub arg: NodeRef,
}

/// One named value-flow-into-composite bridge: a `new` (composite) node, the
/// field/property/named-argument name it fills, and the value node. The pseudo
/// field `..` records a spread / functional-update base.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct DfField {
    pub owner: NodeRef,
    pub name: String,
    pub value: NodeRef,
}

/// One string-carrying df node's text row: `kind` is lit|template|concat and
/// `text` is the cooked literal value (`lit`) or the raw source slice
/// (`template`/`concat` — a syntactic label, never a type judgment). Port of
/// v5 `DataflowFacts::lits`.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct DfLit {
    pub node: NodeRef,
    pub kind: &'static str,
    pub text: String,
}

/// Port of v5 `LoopFact` (src/graph/typegraph/mod.rs:390-397); lines become the
/// byte span, `fn_sym` drops out (span containment carries it). None == v5 `""`.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct DfLoop {
    pub span: Span,
    pub var: Option<String>,
    /// The iterated expression's raw source slice. v5 leaves this empty at every
    /// push site; its own `NestFact` comment calls that "until extractors fill it".
    pub collection: Option<String>,
}

/// Port of v5 `NestFact` (src/graph/typegraph/mod.rs:405-410). `loop_span`
/// replaces v5's `"{file}:{start}"` loop_id; the file is the row's own file.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct DfNest {
    pub call: NodeRef,
    pub loop_span: Span,
    /// 1 = the outermost enclosing loop.
    pub depth: u32,
    pub collection: Option<String>,
}

/// One callable whose body builds a collection, RUST ONLY (v5 fills
/// `allocators` at rust/mod.rs:1149 and :1176 and nowhere else).
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct DfAllocates {
    /// The fn/method item's span, or the closure's when the allocating call sits
    /// in one: v5 keys on `fn_sym`, which a closure rebinds to its `lam_sym`.
    pub owner: Span,
}

/// DfF side-channel rows that cannot be represented as uniform node/edge rows.
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct DfFAux {
    pub params: Vec<DfParam>,
    pub args: Vec<DfArg>,
    pub fields: Vec<DfField>,
    pub lits: Vec<DfLit>,
    pub loops: Vec<DfLoop>,
    pub nests: Vec<DfNest>,
    pub allocates: Vec<DfAllocates>,
    /// Pending (node, start, end, kind) spans for `template`/`concat` rows,
    /// whose text is a source SLICE the per-node lift doesn't hold. The ts
    /// DfF projector drains this into `lits` once, at the end of the walk, where
    /// the file content is in hand (the same shape as v5's `lit_spans`).
    pub lit_spans: Vec<(NodeRef, u32, u32, &'static str)>,
    /// Pending `(index into loops, start, end)`, drained into
    /// `loops[index].collection` by the ts and rust projectors (`lit_spans` shape).
    pub loop_collection_spans: Vec<(usize, u32, u32)>,
    /// Scratch allocator-call spans, claimed by the INNERMOST enclosing callable:
    /// the rust closure arm and `project_df` roll their range up and truncate.
    pub allocator_hits: Vec<Span>,
}

/// Port of v5 `compute_nests` (src/graph/typegraph/mod.rs:872-905). Byte-span
/// containment replaces v5's `fn_sym` + `::closure::` ancestry test (:876-884).
pub fn compute_nests(nodes: &[Node<DfF>], loops: &[DfLoop]) -> Vec<DfNest> {
    let mut out = Vec::new();
    let mut enclosing: Vec<&DfLoop> = Vec::new();
    for (index, node) in nodes.iter().enumerate() {
        // `new` nodes count too: a constructor in a loop allocates per iteration.
        if !matches!(node.kind, DfNodeKind::CallRes | DfNodeKind::New) {
            continue;
        }
        enclosing.clear();
        enclosing.extend(loops.iter().filter(|enclosing_loop| {
            enclosing_loop.span.start <= node.span.start
                && node.span.end() <= enclosing_loop.span.end()
        }));
        enclosing.sort_by_key(|enclosing_loop| enclosing_loop.span.start);
        for (rank, enclosing_loop) in enclosing.iter().enumerate() {
            out.push(DfNest {
                call: NodeRef(index as u32),
                loop_span: enclosing_loop.span,
                depth: rank as u32 + 1,
                collection: enclosing_loop.collection.clone(),
            });
        }
    }
    out
}

/// df_node kind. Core = every variant at least two languages construct today
/// (or none yet: `Try`); a kind one language owns lives in that language's
/// file as an `Ext(LangKind)` constant (rust.rs BORROW/MATCH/BLOCK/BREAK,
/// ts.rs COND/CONCAT/TEMPLATE).
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
    Binop,
    Unop,
    Loop,
    If,
    Closure,
    Try,
    Expr,
    Logic,
    Ext(LangKind),
}

impl DfNodeKind {
    pub const fn as_str(self) -> &'static str {
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
            DfNodeKind::Binop => "binop",
            DfNodeKind::Unop => "unop",
            DfNodeKind::Loop => "loop",
            DfNodeKind::If => "if",
            DfNodeKind::Closure => "closure",
            DfNodeKind::Try => "try",
            DfNodeKind::Expr => "expr",
            DfNodeKind::Logic => "logic",
            DfNodeKind::Ext(ext) => ext.tag,
        }
    }
}

/// df_edge kind. `Direct` is v5's unkinded df_edge(from,to). Cross-function
/// value edges live in the `FlowF` family, not here (own plane, own closure).
#[derive(Copy, Clone, Debug, PartialEq, Eq)]
pub enum DfEdgeKind {
    /// An intra-procedural value edge: dst receives the value of src.
    Direct,
}

impl DfEdgeKind {
    pub const fn as_str(self) -> &'static str {
        match self {
            DfEdgeKind::Direct => "direct",
        }
    }
}

impl Family for DfF {
    type NodeKind = DfNodeKind;
    type EdgeKind = DfEdgeKind;
    type Aux = DfFAux;
    const TAG: FamilyTag = FamilyTag::Df;
}

// ── VALUE-FLOW plane: FlowF  (inter-procedural value flow) ───────────────────

/// Cross-function value flow, a separate family from `DfF`. Phase-2 only: no
/// `FamilyMask` bit, no `ExtractOutput` field; a pure join computes its edges.
#[derive(Default, Copy, Clone, Debug)]
pub struct FlowF;

/// Cross-function value edge kind. `DfDirect` is absent: that is DfF's plane.
#[derive(Copy, Clone, Debug, PartialEq, Eq)]
pub enum FlowEdgeKind {
    /// A caller argument value flows into the callee's parameter at the same
    /// positional slot.
    ArgToParam,
    /// A callee return value reaches the caller's call-result node. The edge is
    /// caller-local, so the VALUE travels dst to src for this kind alone.
    RetToCallRes,
    /// A captured value flows into the closure's element slot.
    LambdaElem,
    /// A closure's return value flows out to the closure node.
    LambdaRet,
}

impl FlowEdgeKind {
    pub const fn as_str(self) -> &'static str {
        match self {
            FlowEdgeKind::ArgToParam => "arg_to_param",
            FlowEdgeKind::RetToCallRes => "ret_to_call_res",
            FlowEdgeKind::LambdaElem => "lambda_elem",
            FlowEdgeKind::LambdaRet => "lambda_ret",
        }
    }
}

impl Family for FlowF {
    type NodeKind = DfNodeKind;
    type EdgeKind = FlowEdgeKind;
    type Aux = ();
    const TAG: FamilyTag = FamilyTag::Flow;
}

/// One cross-function value-flow edge, BOTH endpoints (blob, span) because flow
/// crosses files. Emitted only by the `flow_edges` join.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct FlowEdge {
    pub src_blob: ContentId,
    pub src_span: Span,
    pub dst_blob: ContentId,
    pub dst_span: Span,
    pub kind: FlowEdgeKind,
}

/// The pure inter-procedural value-flow join: `DfArg` x resolved call edge x
/// `DfParam` (ArgToParam) plus callee `Ret` nodes (RetToCallRes).
pub fn flow_edges(
    inputs: &[(ContentId, &ExtractOutput)],
    resolved: &[(ContentId, Vec<ProjectEdge<CallF>>)],
) -> Vec<FlowEdge> {
    let by_blob: std::collections::HashMap<ContentId, &ExtractOutput> = inputs
        .iter()
        .map(|(blob, out)| (blob.clone(), *out))
        .collect();
    let mut edges = Vec::new();
    for (caller_blob, call_edges) in resolved {
        let Some(caller) = by_blob.get(caller_blob) else {
            continue;
        };
        let Some(caller_df) = caller.df.as_ref() else {
            continue;
        };
        for call_edge in call_edges {
            let Some(site) = call_edge.call_site else {
                continue;
            };
            let Some(call_node) = call_node(caller_df, site) else {
                continue;
            };
            let Some(callee) = by_blob.get(&call_edge.dst_blob) else {
                continue;
            };
            let Some(callee_df) = callee.df.as_ref() else {
                continue;
            };
            for arg in &caller_df.aux.args {
                if arg.call != call_node || arg.pos < 0 {
                    continue;
                }
                for param in &callee_df.aux.params {
                    let param_span = callee_df.node(param.node).span;
                    let in_callee = call_edge.dst_span.start <= param_span.start
                        && param_span.end() <= call_edge.dst_span.end();
                    if !in_callee || param.pos as i64 != arg.pos {
                        continue;
                    }
                    edges.push(FlowEdge {
                        src_blob: caller_blob.clone(),
                        src_span: caller_df.node(arg.arg).span,
                        dst_blob: call_edge.dst_blob.clone(),
                        dst_span: param_span,
                        kind: FlowEdgeKind::ArgToParam,
                    });
                }
            }
            let call_span = caller_df.node(call_node).span;
            for node in &callee_df.nodes {
                if node.kind != DfNodeKind::Ret {
                    continue;
                }
                let in_callee = call_edge.dst_span.start <= node.span.start
                    && node.span.end() <= call_edge.dst_span.end();
                if !in_callee {
                    continue;
                }
                edges.push(FlowEdge {
                    src_blob: caller_blob.clone(),
                    src_span: call_span,
                    dst_blob: call_edge.dst_blob.clone(),
                    dst_span: node.span,
                    kind: FlowEdgeKind::RetToCallRes,
                });
            }
        }
    }
    edges
}

/// The caller's call node at `site`: the `CallRes`/`New` node whose span equals
/// the site, else the smallest such span containing it, else `None`.
fn call_node(bundle: &FamilyBundle<DfF>, site: Span) -> Option<NodeRef> {
    let is_call = |kind: DfNodeKind| matches!(kind, DfNodeKind::CallRes | DfNodeKind::New);
    for (index, node) in bundle.nodes.iter().enumerate() {
        if is_call(node.kind) && node.span == site {
            return Some(NodeRef(index as u32));
        }
    }
    let mut best: Option<(Span, NodeRef)> = None;
    for (index, node) in bundle.nodes.iter().enumerate() {
        if !is_call(node.kind) {
            continue;
        }
        let contains = node.span.start <= site.start && site.end() <= node.span.end();
        let tighter = best.map_or(true, |(span, _)| {
            node.span.end() - node.span.start < span.end() - span.start
        });
        if contains && tighter {
            best = Some((node.span, NodeRef(index as u32)));
        }
    }
    best.map(|(_, node)| node)
}

// ── CONTROL-FLOW plane: CfgF ────────────────────────────────────────────────

/// Intra-procedural control flow, DERIVED from the CstF parse: an Entry/Exit
/// pair per callable plus one node per control point.

/// The only per-language input is the `kind_role` table in `crate::cfg`.
#[derive(Default, Copy, Clone, Debug)]
pub struct CfgF;

/// cfg_node kind. Entry and Exit are the callable's two synthetic endpoints and
/// both carry ITS span, so kind is what separates them.
#[derive(Copy, Clone, Debug, PartialEq, Eq)]
pub enum CfgNodeKind {
    Entry,
    Exit,
    Stmt,
    Branch,
    Loop,
    Jump,
    Ret,
}

impl CfgNodeKind {
    pub const fn as_str(self) -> &'static str {
        match self {
            CfgNodeKind::Entry => "entry",
            CfgNodeKind::Exit => "exit",
            CfgNodeKind::Stmt => "stmt",
            CfgNodeKind::Branch => "branch",
            CfgNodeKind::Loop => "loop",
            CfgNodeKind::Jump => "jump",
            CfgNodeKind::Ret => "ret",
        }
    }
}

/// cfg_edge kind. `Next` is plain succession, a loop's back edge included (the
/// back edge is the one whose dst starts before its src).

/// `Arm` enters a branch arm or a loop body, `Jump` leaves a break/continue,
/// `Exit` enters the callable's Exit node.
#[derive(Copy, Clone, Debug, PartialEq, Eq)]
pub enum CfgEdgeKind {
    Next,
    Arm,
    Jump,
    Exit,
}

impl CfgEdgeKind {
    pub const fn as_str(self) -> &'static str {
        match self {
            CfgEdgeKind::Next => "next",
            CfgEdgeKind::Arm => "arm",
            CfgEdgeKind::Jump => "jump",
            CfgEdgeKind::Exit => "exit",
        }
    }
}

impl Family for CfgF {
    type NodeKind = CfgNodeKind;
    type EdgeKind = CfgEdgeKind;
    type Aux = ();
    const TAG: FamilyTag = FamilyTag::Cfg;
}

// ── DATA plane: DataF ───────────────────────────────────

/// json / jsonl / yaml / toml as ONE plane, v5 `src/datapath.rs` ported: one
/// grammar per extension, every hit with a byte span. A row TABLE, not a graph,
/// so `nodes` and `edges` stay empty and every row rides `DataFAux`.
#[derive(Default, Copy, Clone, Debug)]
pub struct DataF;

/// Which grammar read the file, from its extension (v5 `datapath.rs` `fmt_of`).
#[derive(Copy, Clone, Debug, Default, PartialEq, Eq)]
pub enum DataFormat {
    #[default]
    Json,
    Jsonl,
    Yaml,
    Toml,
}

impl DataFormat {
    /// `.jsonl`/`.ndjson` -> Jsonl, `.yaml`/`.yml` -> Yaml, `.toml` -> Toml,
    /// anything else -> Json (v5 `fmt_of`, datapath.rs:18-25).
    pub fn of_path(path: &str) -> Self {
        match path.rsplit('.').next().unwrap_or("") {
            "jsonl" | "ndjson" => DataFormat::Jsonl,
            "yaml" | "yml" => DataFormat::Yaml,
            "toml" => DataFormat::Toml,
            _ => DataFormat::Json,
        }
    }

    pub const fn as_str(self) -> &'static str {
        match self {
            DataFormat::Json => "json",
            DataFormat::Jsonl => "jsonl",
            DataFormat::Yaml => "yaml",
            DataFormat::Toml => "toml",
        }
    }
}

/// The value classes every data document is built from.
#[derive(Copy, Clone, Debug, PartialEq, Eq)]
pub enum DataValueKind {
    Object,
    Array,
    String,
    Number,
    Boolean,
    Null,
}

impl DataValueKind {
    pub const fn as_str(self) -> &'static str {
        match self {
            DataValueKind::Object => "object",
            DataValueKind::Array => "array",
            DataValueKind::String => "string",
            DataValueKind::Number => "number",
            DataValueKind::Boolean => "boolean",
            DataValueKind::Null => "null",
        }
    }
}

/// One document of a file: a yaml stream yields one per `---` document, a jsonl
/// file one per non-empty line, json and toml exactly one. `ordinal` is the
/// surrogate key every `DataValueRow` of the document joins on. `value` is the
/// document as a json VALUE built from the same parse the rows come from, and it
/// is the column dl6's `decode/2` brace pattern reads.
#[derive(Clone, Debug, PartialEq)]
pub struct DataDoc {
    pub ordinal: u32,
    pub span: Span,
    pub value: serde_json::Value,
}

/// One value inside a document. `path` is v5's dotted address (`paths./pets.get`,
/// array indices as decimal, a toml dotted key expanded to one segment each);
/// the root value's path is the empty string. `text` is the scalar's unquoted,
/// unescaped source text and is None for `Object`/`Array`, whose `span` already
/// delimits the whole subtree.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct DataValueRow {
    pub doc: u32,
    pub path: NameId,
    pub kind: DataValueKind,
    pub text: Option<NameId>,
    pub span: Span,
}

/// The DataF side-channel: the plane's whole output.
#[derive(Clone, Debug, Default, PartialEq)]
pub struct DataFAux {
    pub format: DataFormat,
    pub docs: Vec<DataDoc>,
    pub values: Vec<DataValueRow>,
}

/// Nothing on this plane relates two nodes; the parent link is spelled by the
/// dotted `path` prefix, never by an edge.
#[derive(Copy, Clone, Debug, PartialEq, Eq)]
pub enum DataEdgeKind {
    Child,
}

impl Family for DataF {
    type NodeKind = DataValueKind;
    type EdgeKind = DataEdgeKind;
    type Aux = DataFAux;
    const TAG: FamilyTag = FamilyTag::Data;
}

// ── RESOLUTION plane: ModuleF  (COLLAPSED BY DECISION - not a family) ───────
// Fork C, chosen by Chris 2026-08-17 (issue extract-modulef-collapse): no new
// family. Phase-1 specifier rows stay in `CallFAux.specifiers`, and v5's
// module-level distinctions come back on the WIRE instead: `file_edge` carries
// the specifier kind, `file_unresolved` carries the stopped specifiers, and
// `package_edge` carries the manifest graph. The Resolve surface (S5 below)
// therefore declares no ModuleF arm. Sketch, kept as the revival shape:
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

/// WHICH resolver leg answered a `ProjectEdge`. CLOSED: a new leg gets a new
/// variant, never a new spelling at the call site.
#[derive(Clone, Copy, Debug, PartialEq, Eq, PartialOrd, Ord)]
pub enum ResolutionOrigin {
    /// The referring file's own scope: the file itself, or its package block
    /// where the language scopes one (go's directory).
    SameFile,
    /// Exactly one corpus declaration carries the name, so the name IS the
    /// coordinate. The weakest leg: it guesses wherever the corpus is unique.
    CorpusUnique,
    /// The language's own import/module plane bound the name.
    ModulePlane,
    /// A real type checker (rust-analyzer) answered for this site.
    Checker,
    /// An alias, re-export or multi-hop selector chain was followed to a def.
    AliasChain,
    /// A parameter's or local binding's declared/inferred type answered.
    Param,
    /// A call's receiver expression was typed, and the type's method set
    /// carried the callee.
    Receiver,
    /// The enclosing impl's self type, or an associated path through it.
    SelfType,
    /// An interface/trait method fanned out to its implementers, or an
    /// implementer was matched back to the interface it satisfies.
    IfaceImpl,
    /// A decorator application rebound the name to what the decorator returns.
    Decorator,
    /// A subscript binding (`table["key"]`) named the callee.
    Subscript,
    /// The callee is the value a already-resolved call returns (`f()()`).
    ReturnCall,
    /// A SCIP index (the compiler's own answer) named the target.
    Scip,
    /// No leg answered; the edge carries a placeholder target, which the flat
    /// wire then drops. Type-edge candidates are the only minters.
    Unresolved,
}

impl ResolutionOrigin {
    pub fn as_str(self) -> &'static str {
        match self {
            ResolutionOrigin::SameFile => "same_file",
            ResolutionOrigin::CorpusUnique => "corpus_unique",
            ResolutionOrigin::ModulePlane => "module_plane",
            ResolutionOrigin::Checker => "checker",
            ResolutionOrigin::AliasChain => "alias_chain",
            ResolutionOrigin::Param => "param",
            ResolutionOrigin::Receiver => "receiver",
            ResolutionOrigin::SelfType => "self_type",
            ResolutionOrigin::IfaceImpl => "iface_impl",
            ResolutionOrigin::Decorator => "decorator",
            ResolutionOrigin::Subscript => "subscript",
            ResolutionOrigin::ReturnCall => "return_call",
            ResolutionOrigin::Scip => "scip",
            ResolutionOrigin::Unresolved => "unresolved",
        }
    }

    /// The envelope's own word for this leg. The two vocabularies are one list
    /// by construction, so a leg added here has a seat there.
    pub fn method(self) -> crate::tsi::types::Method {
        use crate::tsi::types::Method;
        match self {
            ResolutionOrigin::SameFile => Method::SameFile,
            ResolutionOrigin::CorpusUnique => Method::CorpusUnique,
            ResolutionOrigin::ModulePlane => Method::ModulePlane,
            ResolutionOrigin::Checker => Method::Checker,
            ResolutionOrigin::AliasChain => Method::AliasChain,
            ResolutionOrigin::Param => Method::Param,
            ResolutionOrigin::Receiver => Method::Receiver,
            ResolutionOrigin::SelfType => Method::SelfType,
            ResolutionOrigin::IfaceImpl => Method::IfaceImpl,
            ResolutionOrigin::Decorator => Method::Decorator,
            ResolutionOrigin::Subscript => Method::Subscript,
            ResolutionOrigin::ReturnCall => Method::ReturnCall,
            ResolutionOrigin::Scip => Method::Scip,
            ResolutionOrigin::Unresolved => Method::Unresolved,
        }
    }
}

/// A project-phase edge: `dst` lives in ANOTHER blob (resolved across the file
/// set). The seed's `ProjectEdge` (`_0_shape.rs`:222-232) made generic over the
/// family — the seed's `EdgeKind` sum is deleted per D-families, so `kind` is
/// `F::EdgeKind`. Emitted ONLY by `Resolve<F>` (phase 2); the store seam
/// resolves the dst to a `node_id` by joining `(dst_blob, dst_span, kind)`.
#[derive(Clone, Debug)]
pub struct ProjectEdge<F: Family> {
    /// Local node in this file (into the producing file's node vec).
    pub src: NodeRef,
    /// The resolved target's content key.
    pub dst_blob: ContentId,
    /// The target node's coordinate inside `dst_blob`.
    pub dst_span: Span,
    pub kind: F::EdgeKind,
    /// Which resolver leg answered. Constructor-required: a leg that cannot
    /// name itself is a leg nothing can count.
    pub origin: ResolutionOrigin,
    /// Every leg that named this target. EMPTY means `origin` alone, so an
    /// edge nobody asked to witness allocates nothing here.
    pub witnesses: Vec<ResolutionOrigin>,
    /// The phase-1 call site that produced this edge, when the edge is a
    /// resolved CallF row. TypeF and legacy resolver rows leave this empty.
    pub call_site: Option<Span>,
    _f: PhantomData<fn() -> F>,
}

impl<F: Family> ProjectEdge<F> {
    pub fn new(
        src: NodeRef,
        dst_blob: ContentId,
        dst_span: Span,
        kind: F::EdgeKind,
        origin: ResolutionOrigin,
    ) -> Self {
        Self {
            src,
            dst_blob,
            dst_span,
            kind,
            origin,
            witnesses: Vec::new(),
            call_site: None,
            _f: PhantomData,
        }
    }

    pub fn with_call_site(mut self, call_site: Span) -> Self {
        self.call_site = Some(call_site);
        self
    }

    /// A second leg that reached the same target. Idempotent: a leg already in
    /// the list is one witness, never two rows on one fact.
    pub fn witnessed_by(mut self, extra: ResolutionOrigin) -> Self {
        if self.witnesses.is_empty() {
            self.witnesses.push(self.origin);
        }
        if !self.witnesses.contains(&extra) {
            self.witnesses.push(extra);
        }
        self
    }

    /// Every leg that named this target, `origin` first. Allocates, so the
    /// witness path is the only caller.
    pub fn legs(&self) -> Vec<ResolutionOrigin> {
        match self.witnesses.is_empty() {
            true => vec![self.origin],
            false => self.witnesses.clone(),
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

// ── phase 2: the Resolve seam ───────────────────────────────────────────────

/// Borrowed view over one (repo, rev) project, shared across a language's
/// phase-2 calls. Extract is content-local; this is the ONLY handle it gets to
/// the world beyond the blob it was handed. Spec: seed `_2_traits.rs`:29-51
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
    /// Send + Sync so a parallel per-file resolve can share one `&ProjectCx`.
    pub reader: Option<&'a (dyn Fn(&str) -> Option<Vec<u8>> + Send + Sync)>,
    /// The fold of `files` + `manifests` that invalidates phase-2 on change; the
    /// middle component of the phase-2 cache key (see `Resolve`). Spec:
    /// `_2_traits.rs`:44-45.
    pub digest: ProjectDigest,
    /// Lazy, per-language whole-project indexes (v5: RustCrates, ts_packages,
    /// GoIndex). Opaque here; each language module owns its concrete index type
    /// behind a OnceLock. Spec: `_2_traits.rs`:46-51 (field) + :59-61 (IndexBag).
    pub indexes: IndexBag,
    /// Off, a leg that answers after the checker never runs. On, every leg runs
    /// and lands on the edge's `witnesses`, or on an edge of its own.
    pub witness: bool,
}

/// The blob of the output currently being resolved. Thread-local rather than a
/// `ProjectCx` field so the per-file resolve loop can run on the extract pool:
/// each worker pins its own current blob, and a shared `&ProjectCx` stays Sync.
/// `None` in hand-built contexts (unit tests), where `own_blob` falls back to
/// the deterministic span-count rule.
thread_local! {
    static OWN: std::cell::RefCell<Option<ContentId>> = const { std::cell::RefCell::new(None) };
}

/// Pin the calling thread's current blob before a per-file resolve (or clear it
/// after, so pool threads never leak a stale blob into a later task).
pub fn set_own(blob: Option<ContentId>) {
    OWN.with(|own| *own.borrow_mut() = blob);
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
///   means pure name-match resolution (no scip oracle loaded).
/// - `joined_documents`: index x reader, ONE slot; per FILE it is 82x129 reads.
/// - `paths`: blob -> supplied path. ONE lang-agnostic slot for the arms whose
///   rule is about where a file SITS (go binds `pkg.F` in a directory).
/// - `ts_modules`: the ts/js module plane, a per-language slot. ResolveExport
///   is whole-corpus; `Resolve::resolve` runs per file.
#[derive(Default)]
pub struct IndexBag {
    pub def_index: std::sync::OnceLock<DefIndex>,
    pub scip_index: std::sync::OnceLock<ScipIndex>,
    pub joined_documents: std::sync::OnceLock<Vec<Option<(ContentId, Vec<u8>)>>>,
    pub paths: std::sync::OnceLock<PathIndex>,
    pub kinds: std::sync::OnceLock<KindIndex>,
    pub ts_modules: std::sync::OnceLock<crate::lang::ts_resolve::TsModuleIndex>,
    /// the rust module plane, same discipline as `ts_modules`.
    pub rust_modules: std::sync::OnceLock<crate::lang::rust_modules::RustModuleIndex>,
    /// the go module plane, same discipline as `ts_modules`/`rust_modules`.
    pub go_modules: std::sync::OnceLock<crate::lang::go_modules::GoModuleIndex>,
    /// the rust CHECKER tier's answers, joined to corpus def coordinates. Unset
    /// without `--rust-checker`, and unset when the workspace load fell back.
    pub rust_checker: std::sync::OnceLock<crate::lang::rust_checker::RustCheckerIndex>,
    /// the ts CHECKER tier's answers, same discipline as `rust_checker`.
    pub ts_checker: std::sync::OnceLock<crate::lang::ts_checker::TsCheckerIndex>,
}

/// Blob -> supplied path, for the whole resolve universe. Built ONCE per
/// refresh beside the `DefIndex`, from the same inputs; never a re-read.
#[derive(Clone, Debug, Default)]
pub struct PathIndex {
    pub map: std::collections::HashMap<ContentId, String>,
}

impl PathIndex {
    pub fn get(&self, blob: &ContentId) -> Option<&str> {
        self.map.get(blob).map(String::as_str)
    }
}

/// Build the `PathIndex` from the (blob, path) pairs the resolve was handed.
/// Two identical files share one blob; the FIRST path supplied wins.
pub fn build_path_index<'a>(inputs: impl IntoIterator<Item = (ContentId, &'a str)>) -> PathIndex {
    let mut index = PathIndex::default();
    for (blob, path) in inputs {
        index.map.entry(blob).or_insert_with(|| path.to_string());
    }
    index
}

/// Blob + def span -> that def's `CallKind`, for the whole resolve universe.
/// `DefSite` carries the family leg but not the kind, and a receiver-blind
/// name match has to know whether a candidate is a class member.
#[derive(Clone, Debug, Default)]
pub struct KindIndex {
    pub map: std::collections::HashMap<(ContentId, Span), CallKind>,
}

impl KindIndex {
    pub fn get(&self, blob: &ContentId, span: Span) -> Option<CallKind> {
        self.map.get(&(blob.clone(), span)).copied()
    }
}

/// Build the `KindIndex` ONCE per refresh, from the same phase-1 outputs
/// `build_def_index` reads. CallF nodes only: TypeF entities carry a different
/// kind vocabulary.
pub fn build_kind_index(outputs: &[(ContentId, &ExtractOutput)]) -> KindIndex {
    let mut index = KindIndex::default();
    for (blob, output) in outputs {
        if let Some(call) = &output.call {
            for node in &call.nodes {
                index.map.insert((blob.clone(), node.span), node.kind);
            }
        }
    }
    index
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
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct DefSite {
    pub blob: ContentId,
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
    /// Per-blob span index over the SAME sites `map` holds, sorted by span
    /// start, each entry carrying the max end of every entry up to and
    /// including it (the prefix max). `containing_def_site` binary-searches
    /// this instead of scanning every name's site list per call.
    spans: std::collections::HashMap<ContentId, Vec<DefSpanEntry>>,
    /// The name behind each `DefSpanEntry::name_ix` (one String per indexed
    /// name, mirrored from `map`'s keys at build time).
    names: Vec<String>,
}

/// One entry of the per-blob span index. `max_end` is the running max of
/// `span.end()` over the blob's entries [0..=self] in sorted order: once it
/// drops below the probe's end while walking left, no earlier entry contains
/// the probe either.
#[derive(Clone, Debug)]
struct DefSpanEntry {
    span: Span,
    family: FamilyTag,
    name_ix: u32,
    max_end: u32,
}

/// Build THE `DefIndex` ONCE per refresh, from every file's phase-1
/// `ExtractOutput` paired with its blob hash. Never per-lang, NEVER from
/// `ProjectCx.reader` bytes (re-parsing in phase 2 is the triplication the
/// phase split exists to prevent). Reaches `Resolve::resolve` through
/// `ProjectCx.indexes.def_index` — NOT an explicit param: whole-project state
/// built once per refresh is exactly what the cx exists to carry, and a param
/// would invite per-call rebuilds beside the cx.
pub fn build_def_index(outputs: &[(ContentId, &ExtractOutput)]) -> DefIndex {
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
                            blob: blob.clone(),
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
                            blob: blob.clone(),
                            span: node.span,
                            family: TypeF::TAG,
                        });
                }
            }
        }
    }
    index.build_span_index();
    index
}

impl DefIndex {
    /// Fill the per-blob span index from `map`: one entry per site, sorted by
    /// (start, end), each carrying the prefix max end. The name vector is
    /// rebuilt in the same pass, so entries can name their def by index.
    fn build_span_index(&mut self) {
        self.names.clear();
        self.spans.clear();
        let mut spans: std::collections::HashMap<ContentId, Vec<DefSpanEntry>> =
            std::collections::HashMap::new();
        for (name, sites) in &self.map {
            let name_ix = self.names.len() as u32;
            self.names.push(name.clone());
            for site in sites {
                spans
                    .entry(site.blob.clone())
                    .or_default()
                    .push(DefSpanEntry {
                        span: site.span,
                        family: site.family,
                        name_ix,
                        max_end: 0,
                    });
            }
        }
        for entries in spans.values_mut() {
            entries.sort_unstable_by_key(|e| (e.span.start, e.span.end()));
            let mut running_max = 0u32;
            for entry in entries.iter_mut() {
                running_max = running_max.max(entry.span.end());
                entry.max_end = running_max;
            }
        }
        self.spans = spans;
    }
}

/// Caller binding: the CallF def node whose span most tightly CONTAINS `site`
/// (the innermost covering def), by binary search over def spans sorted by
/// (start, end). Written ONCE here, used by all three lang resolve arms —
/// every lang emits body-covering def spans by design, so one sorted-span
/// search serves ts, rust, and go uniformly. Pure fn over the bundle; zero AST.
pub fn covering_def(defs: &FamilyBundle<CallF>, site: Span) -> Option<NodeRef> {
    // One linear pass for the tightest cover, no sort and no allocation. The
    // previous form sorted the whole bundle per call; ties break the same way
    // the sorted order did (min length, then min (start, end), then node order).
    let mut best: Option<(Span, NodeRef)> = None;
    for (ix, node) in defs.nodes.iter().enumerate() {
        let span = node.span;
        if span.start > site.start || site.end() > span.end() {
            continue;
        }
        let key = (span.end() - span.start, span.start, span.end());
        let better = match best {
            None => true,
            Some((b, _)) => {
                let bkey = (b.end() - b.start, b.start, b.end());
                key < bkey
            }
        };
        if better {
            best = Some((span, NodeRef(ix as u32)));
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

/// Which blob produced `output`. When `cx.own` is set (the `resolve_project`
/// path) it IS the answer: the caller knows which output it handed in, and any
/// span search is a guess. The fallback (hand-built contexts) counts, per
/// blob, how many of the output's named spans match a `DefSite` in the index
/// and returns the single highest-count blob; a tie means the index cannot
/// distinguish the files, so None. Blobs are scored in sorted `ContentId`
/// order so the fallback stays stable under any later tie-break. One pass
/// over the index, never one pass per named span.
pub fn own_blob(cx: &ProjectCx, output: &ExtractOutput) -> Option<ContentId> {
    if let Some(own) = OWN.with(|own| own.borrow().clone()) {
        return Some(own);
    }
    let index = cx.indexes.def_index.get()?;
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
    let mut counts: Vec<(ContentId, usize)> = Vec::new();
    for sites in index.map.values() {
        for site in sites {
            if named_spans.binary_search(&site.span).is_ok() {
                match counts.iter_mut().find(|(blob, _)| *blob == site.blob) {
                    Some((_, count)) => *count += 1,
                    None => counts.push((site.blob.clone(), 1)),
                }
            }
        }
    }
    if counts.len() < 2 {
        return counts.into_iter().next().map(|(blob, _)| blob);
    }
    counts.sort_by(|a, b| (b.1, &a.0).cmp(&(a.1, &b.0)));
    let (top_blob, top_count) = &counts[0];
    if counts[1].1 == *top_count {
        return None;
    }
    Some(top_blob.clone())
}

/// The corpus def site in `blob` whose node span CONTAINS `span` (the scip
/// def-occurrence join: scip's def range marks the def IDENTIFIER, which sits
/// inside v6's whole-declaration def span). Prefers the CallF facet (a call
/// edge binds a callable), then the smallest containing span (the innermost
/// def). Returns the index's map-key name + the site. Pure fn over the index;
/// zero AST. Written ONCE here for the ts/rust/go resolve arms (4c/4d).
pub fn containing_def_site(
    index: &DefIndex,
    blob: ContentId,
    span: Span,
) -> Option<(&str, DefSite)> {
    containing_def_site_in(index, blob, span, None)
}

/// The same containment join with one name excluded (`containing_ts_def`
/// skips the module-synthesis name). The search is a binary search over the
/// blob's span-sorted entries (built by `build_def_index`) plus a bounded
/// leftward walk whose prefix-max-end prune guarantees every visited entry
/// could contain `span`: candidate starts all sit at or before `span.start`,
/// and once the running max end of everything at or before the cursor drops
/// below `span.end()` no earlier entry contains it either.
pub fn containing_def_site_in<'a>(
    index: &'a DefIndex,
    blob: ContentId,
    span: Span,
    skip_name: Option<&str>,
) -> Option<(&'a str, DefSite)> {
    let entries = index.spans.get(&blob)?;
    // Every container has span.start <= probe.start, so candidates live in
    // entries[..p). Walk left from the innermost candidate.
    let p = entries.partition_point(|entry| entry.span.start <= span.start);
    let mut best: Option<(&str, DefSite)> = None;
    for entry in entries[..p].iter().rev() {
        if entry.max_end < span.end() {
            break;
        }
        if entry.span.end() < span.end() {
            continue;
        }
        let name = index.names[entry.name_ix as usize].as_str();
        if skip_name == Some(name) {
            continue;
        }
        let better = match &best {
            None => true,
            Some((_, b)) => {
                let call_bias = (entry.family == CallF::TAG, b.family == CallF::TAG);
                call_bias.0 && !call_bias.1
                    || (call_bias.0 == call_bias.1
                        && entry.span.end() - entry.span.start < b.span.end() - b.span.start)
            }
        };
        if better {
            best = Some((
                name,
                DefSite {
                    blob: blob.clone(),
                    span: entry.span,
                    family: entry.family,
                },
            ));
        }
    }
    best
}

/// Phase 2: "here is a codebase, get data" — the cross-file resolution half of
/// a `Source` (spec: seed `_2_traits.rs`:80-97 `ProjectExtract`, adapted to the
/// crate's type-level families + Epic U's uniform surface).
///
/// CACHE KEY (vs phase 1): phase 2 keys on `(ContentId, ProjectDigest,
/// FamilyMask)` where phase 1 keys on `(ContentId, lang, FamilyMask)`. Identical
/// BYTES anywhere extract once, but a file appearing/disappearing can change a
/// resolution, so the project digest rides the phase-2 key (spec:
/// `_2_traits.rs`:9-15,80-84; `_7_tasks.rs`:37-38).
///
/// WHICH FAMILIES RESOLVE (spec `_2_traits.rs`:80-84): `TypeF` (field / impl /
/// variant / generic / uses + the resolved param/returns binding) and `CallF`
/// (resolved caller -> callee). MODULE never resolves through an arm: ModuleF is
/// collapsed by decision (S2 above, fork C), and the module plane's answers
/// arrive as the `file_edge` / `file_unresolved` / `package_edge` rows. `DfF` and
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
/// `resolve` has no default body: a `Source` with no plane for `F` implements
/// nothing for `F`; there is no empty default, so a missing arm is a compile
/// error, never an empty result.
// @comment-ok: the no-default constraint is prose the signature alone cannot show
pub trait Resolve<F: Family>: Source {
    /// Turn this file's phase-1 specifiers/names into resolved, cross-file
    /// `ProjectEdge`s. The return is ONLY the cross-file resolutions for this
    /// one blob (spec: `_2_traits.rs`:88-96).
    fn resolve(&self, output: &ExtractOutput, cx: &ProjectCx) -> Vec<ProjectEdge<F>>;
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

impl PositionEncoding {
    /// The scip.proto enum ordinal, for the wire row that reports which
    /// encoding a document's ranges were written in.
    pub fn ordinal(self) -> i32 {
        match self {
            Self::Unspecified => 0,
            Self::Utf8 => 1,
            Self::Utf16 => 2,
            Self::Utf32 => 3,
        }
    }
}

/// An interned scip symbol: an index into `ScipIndex::symbols`, minted at
/// decode. One copy of each distinct symbol string serves every occurrence,
/// symbol information and relationship that references it (a 950k-occurrence
/// index holds ~30k distinct symbols; per-occurrence `String`s held 75 MB of
/// duplicated text on the go corpus).
#[derive(Copy, Clone, Debug, Default, PartialEq, Eq, Hash, PartialOrd, Ord)]
pub struct SymbolId(pub u32);

/// The decode-time interner: dedupes symbol strings into `SymbolId`s and
/// yields the finished `ScipIndex::symbols` table.
#[derive(Default)]
pub struct SymbolInterner {
    ids: std::collections::HashMap<String, SymbolId>,
    table: Vec<String>,
}

impl SymbolInterner {
    /// Mint (or reuse) the id for one symbol string.
    pub fn intern(&mut self, symbol: impl Into<String>) -> SymbolId {
        let symbol = symbol.into();
        let next = SymbolId(self.table.len() as u32);
        *self.ids.entry(symbol.clone()).or_insert_with(|| {
            self.table.push(symbol);
            next
        })
    }

    /// The finished table: `ScipIndex::symbols`, `SymbolId`s index into it.
    pub fn table(self) -> Vec<String> {
        self.table
    }
}

/// One occurrence: a (symbol, range, roles) triple — a definition or a
/// reference site (seed `_4_scip.rs`:26-35). `range` is scip.proto's packed
/// quad normalized to `[start_line, start_col, end_line, end_col]` (the 3-
/// element short form expanded), 0-based, cols in the document's
/// `PositionEncoding` — deliberately NOT a v6 byte `Span`: the line/col ->
/// byte bridge needs the document's content, which only the consumer holds
/// (the ratchet / the engine). `crate::scip::byte_range` is that bridge.
///
/// PASSTHROUGH (scip-passthrough lane): `syntax_kind`, `enclosing_range`,
/// `override_documentation` and `diagnostics` used to be dropped here, which
/// made them inexpressible from a v6 index no matter what a dl rule asked
/// for. A field the protobuf carries now reaches the wire; what to do with it
/// is the dl layer's call.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct ScipOccurrence {
    /// The interned symbol; resolve the text with `ScipIndex::symbol`.
    pub symbol: SymbolId,
    pub range: [i32; 4],
    pub roles: OccurrenceRole,
    /// scip.proto `SyntaxKind` ordinal (0 = UnspecifiedSyntaxKind).
    pub syntax_kind: i32,
    /// The nearest non-trivial enclosing AST node's range, same normalized
    /// quad as `range`. `None` when the indexer emitted none.
    pub enclosing_range: Option<[i32; 4]>,
    /// Range-specific CommonMark docs overriding the symbol's own.
    pub override_documentation: Vec<String>,
    /// Compiler diagnostics the indexer reported at this exact range.
    pub diagnostics: Vec<ScipDiagnostic>,
}

/// One scip.proto `Diagnostic`, reported at an occurrence's range. `severity`
/// and `tags` stay raw enum ordinals; naming them is a dl-layer lookup.
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct ScipDiagnostic {
    pub severity: i32,
    pub code: String,
    pub message: String,
    pub source: String,
    pub tags: Vec<i32>,
}

/// One scip.proto `Signature`: a symbol's rendered signature plus the
/// occurrences inside it that reference other symbols. Those ranges are
/// relative to `text`, never to a document, which is why they travel with the
/// signature instead of joining the document's occurrence rows.
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct ScipSignature {
    pub language: String,
    pub text: String,
    pub occurrences: Vec<ScipOccurrence>,
}

/// Diet `SymbolInformation` (seed `_4_scip.rs`:37-44): the identity string +
/// display name + raw kind ordinal, plus everything the protobuf carries
/// beside them. Docs and signature documentation were dropped by the original
/// diet and are passed through as of the scip-passthrough lane.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct ScipSymbolInfo {
    /// The interned symbol; resolve the text with `ScipIndex::symbol`.
    pub symbol: SymbolId,
    pub display_name: String,
    pub kind: i32,
    /// Relationships to other symbols (implements / type-definition /
    /// references / defines). RETAINED as of the extractor final-form lane:
    /// the diet used to drop these, which made v5's `scip_impl` and the
    /// `scip_edge` family inexpressible from a v6 index. They are raw index
    /// facts and the joins over them belong in the dl layer, not here.
    pub relationships: Vec<ScipRelationship>,
    /// Markdown docstrings, one string per entry the indexer emitted.
    pub documentation: Vec<String>,
    /// The rendered type signature, when the indexer emits one.
    pub signature: Option<ScipSignature>,
    /// The owning symbol of a LOCAL symbol; empty for global symbols, whose
    /// owner is parsed out of the symbol string's own descriptor grammar.
    pub enclosing_symbol: String,
}

/// One SCIP relationship row: this symbol relates to `symbol` in one or more of
/// four ways. The four flags are not exclusive; scip.proto sets several at once
/// (an overriding method is both a reference and an implementation).
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct ScipRelationship {
    /// The interned related symbol; resolve the text with `ScipIndex::symbol`.
    pub symbol: SymbolId,
    pub is_reference: bool,
    pub is_implementation: bool,
    pub is_type_definition: bool,
    pub is_definition: bool,
}

/// One indexed document: path relative to the indexed root + occurrences +
/// symbol infos (seed `_4_scip.rs`:96-104). NO blob leg: the content join
/// (relative_path -> reader/content) is the consumer's — the seed's
/// `ScipDocument.blob` is a join product, not a parse product.
#[derive(Clone, Debug, Default)]
pub struct ScipDocument {
    pub relative_path: String,
    pub position_encoding: PositionEncoding,
    pub occurrences: Vec<ScipOccurrence>,
    pub symbols: Vec<ScipSymbolInfo>,
    /// scip.proto `Language` as the indexer spelled it ("TypeScript", "Go").
    pub language: String,
    /// The document's own text. Indexers leave it empty by default and expect
    /// the client to read the file; it is set for virtual/in-memory documents,
    /// where the file system has no copy to read.
    pub text: String,
    /// The occurrence span table (`crate::scip::DocSpans`): the byte span of
    /// every convertible occurrence, sorted by (start, end), plus the
    /// document's line table. Built on the first `site_occurrence` call
    /// against this document and stored HERE, so two indexes in one process
    /// never share a table. Lazy because the spans need the document's
    /// content, which only the consumer holds.
    pub spans: std::sync::OnceLock<crate::scip::DocSpans>,
}

impl PartialEq for ScipDocument {
    fn eq(&self, other: &Self) -> bool {
        // The span table is a content-derived cache: two documents equal in
        // their parsed fields answer equally no matter which content each
        // was joined against.
        self.relative_path == other.relative_path
            && self.position_encoding == other.position_encoding
            && self.occurrences == other.occurrences
            && self.symbols == other.symbols
            && self.language == other.language
            && self.text == other.text
    }
}

impl Eq for ScipDocument {}

/// One parsed index.scip: the index metadata + documents + the symbols the
/// corpus references and does not define.
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct ScipIndex {
    pub documents: Vec<ScipDocument>,
    pub external_symbols: Vec<ScipSymbolInfo>,
    pub metadata: ScipMetadata,
    /// The symbol interner table built at decode; `SymbolId`s index into it.
    pub symbols: Vec<String>,
    /// symbol -> (document ix, occurrence ix) for the first definition-role
    /// occurrence, first-wins in document order. Filled at the end of
    /// `scip_decode::load_index`; a hand-built index (tests, fixtures) fills
    /// it on the first `definition_of` call instead.
    pub defs: std::sync::OnceLock<DefMap>,
}

/// symbol -> (document ix, occurrence ix) for the first definition-role
/// occurrence, first-wins in document order — the same resolution
/// `definition_of` answers by scan.
pub type DefMap = HashMap<SymbolId, (usize, u32)>;

impl ScipIndex {
    /// The producing indexer's identity, "name version" (the ledger line the
    /// parity goldens print). Derived from metadata rather than stored twice.
    pub fn tool(&self) -> String {
        format!("{} {}", self.metadata.tool_name, self.metadata.tool_version)
    }

    /// The text behind an interned symbol. Unknown ids (a hand-built index
    /// with an empty table) read as the empty string, never a panic.
    pub fn symbol(&self, id: SymbolId) -> &str {
        self.symbols
            .get(id.0 as usize)
            .map(String::as_str)
            .unwrap_or("")
    }
}

/// scip.proto `Metadata` + its nested `ToolInfo`, flattened: one row per
/// index. `version` and `text_document_encoding` stay raw enum ordinals.
/// `project_root` is the file:// URL the document paths hang off, which is the
/// only place an index states what corpus it describes.
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct ScipMetadata {
    pub version: i32,
    pub tool_name: String,
    pub tool_version: String,
    pub tool_arguments: Vec<String>,
    pub project_root: String,
    pub text_document_encoding: i32,
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
    pub data: bool,
}

impl FamilyMask {
    pub const ALL: Self = Self {
        cst: true,
        types: true,
        call: true,
        df: true,
        data: true,
    };
    pub const NONE: Self = Self {
        cst: false,
        types: false,
        call: false,
        df: false,
        data: false,
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
    pub data: Option<FamilyBundle<DataF>>,
}

/// One language binding: a Parser + its per-family Project<F>s behind one masked
/// extract. The v5 TypeLang analog. Held &'static in the roster; no mutable state.
pub trait Source: Sync + Send {
    fn name(&self) -> &'static str;
    fn matches(&self, path: &str) -> bool;
    /// One parse per backing engine, masked projections. Owns the arena(s)
    /// internally; returns owned output (no borrowed parse crosses the seam).
    fn extract(&self, path: &str, content: &[u8], mask: FamilyMask) -> ExtractOutput;
    /// The `ExtractLang` this source parses `path` with. Default: the ast-grep
    /// shim (its `SupportLang` picks the grammar from the path); a language
    /// with its own grammar overrides.
    fn extract_lang(&self, path: &str) -> Option<ExtractLang> {
        SupportLang::from_path(path).map(ExtractLang::Sg)
    }
}

// ── the Rehome seam: what one language answers when a file moves ────────────

/// One import-shaped reference a move respells. `literal` and `text` cover it
/// AS WRITTEN, quotes included: a respell reproduces the quote style.
pub struct ImportRef {
    /// Project-relative path of the file that writes the reference.
    pub importer: String,
    pub literal: Span,
    /// The bytes `literal` spans.
    pub text: String,
    /// Project-relative path the reference names, pre-move.
    pub target: String,
    pub kind: ImportRefKind,
}

/// The vocabulary of one `ImportRef`. Core = the kinds two or more languages
/// construct today; a kind one language owns lives in that language's rehome
/// file as an `Ext(LangKind)` constant, so a new language never edits this list.
#[derive(Copy, Clone, Debug, PartialEq, Eq, Hash)]
pub enum ImportRefKind {
    /// A module import (`use`, `import`, `:- use_module`).
    Import,
    /// A quoted path literal outside an import form.
    PathLiteral,
    /// A package.json / Cargo.toml target line.
    ManifestTarget,
    /// A kind one language owns; tag never equals a core tag (railed in
    /// tests/7_import_ref_kind.rs), so `as_str` stays injective.
    Ext(LangKind),
}

impl ImportRefKind {
    pub const fn as_str(self) -> &'static str {
        match self {
            ImportRefKind::Import => "import",
            ImportRefKind::PathLiteral => "path_literal",
            ImportRefKind::ManifestTarget => "manifest_target",
            ImportRefKind::Ext(ext) => ext.tag,
        }
    }
}

/// One respelled literal: the bytes soopy's Replace writes.
pub struct Respell {
    pub file: String,
    pub span: Span,
    pub text: String,
    /// The stdout line this respell reports itself with. soopy's own preview
    /// covers a staged edit, so only a report a preview does not carry is set.
    pub receipt: Option<String>,
}

/// What one language answers when a file it owns moves. Held `&'static` in the
/// `rehomes()` roster beside `sources()`; one impl per language, no mutable state.
pub trait Rehome: Source + Sync + Send {
    /// Every reference this language owns that `cx`'s batch can reach, one parse
    /// per file. Batch-gated: a resolver call is a syscall per specifier.
    fn import_refs(&self, cx: &MoveCx) -> Vec<ImportRef>;

    /// The literal text for `reference` once `cx`'s batch lands (importer AND
    /// target may both move). None = unchanged.
    fn respell(&self, cx: &MoveCx, reference: &ImportRef) -> Option<Respell>;

    /// The file name whose stem stands for its directory ("mod" for Rust,
    /// "index" for TS). None: no directory-standing file in this language.
    fn directory_stem(&self) -> Option<&'static str> {
        None
    }

    /// The names a batch can be reached by: every moved file's stem, plus the
    /// directory name of a moved directory-standing file, which is the module
    /// name a decl (or a directory-form specifier) spells.
    fn moved_names(&self, cx: &MoveCx) -> BTreeSet<String> {
        let mut names = BTreeSet::new();
        for old in cx.moved().keys() {
            if !crate::move_cx::owned_by(old, self) {
                continue;
            }
            let own = crate::move_cx::stem(old);
            if Some(own.as_str()) == self.directory_stem() {
                names.insert(crate::move_cx::stem(crate::move_cx::dirname(old)));
            }
            names.insert(own);
        }
        names
    }
}

/// The manifest leg of a move: languages whose package files name paths
/// (Cargo.toml, package.json).
pub trait RehomeManifests: Sync + Send {
    /// The manifest carriers this language owns, project-relative, in path order.
    fn manifests(&self, cx: &MoveCx) -> Vec<String>;

    /// Manifest targets, as `ImportRef`s of kind `ManifestTarget`, so the one
    /// `respell` arm handles them too.
    fn manifest_refs(&self, cx: &MoveCx) -> Vec<ImportRef>;
}

/// The shim leg of a move: a reexport module left at the old path.
pub trait RehomeShim: Sync + Send {
    /// None when `old` cannot be read.
    fn shim(&self, cx: &MoveCx, old: &str, new: &str) -> Option<String>;
}

/// The text-refs leg of a move: spellings a build output wears beyond the
/// source path itself.
pub trait RehomeTextSpellings: Sync + Send {
    /// Extra `(old, new)` pairs for the `--text-refs` report to scan plain text for.
    fn text_spellings(&self, cx: &MoveCx, old: &str, new: &str) -> Vec<(String, String)>;
}

/// The plan-check leg of a move: reasons a batch cannot be planned at all.
pub trait RehomePlanCheck: Sync + Send {
    /// Any row stops the run before a stage is built; the core never sees a
    /// panic from an arm.
    fn plan_errors(&self, cx: &MoveCx) -> Vec<String>;
}

/// One `rehomes()` roster row: the core every language answers, and the legs
/// only some languages carry. A `None` leg is the language saying "no such
/// thing here", visible in the roster rather than hidden in a default method.
#[derive(Clone, Copy)]
pub struct RehomeArm {
    pub core: &'static dyn Rehome,
    pub manifests: Option<&'static dyn RehomeManifests>,
    pub shim: Option<&'static dyn RehomeShim>,
    pub text_spellings: Option<&'static dyn RehomeTextSpellings>,
    pub plan_check: Option<&'static dyn RehomePlanCheck>,
}

impl RehomeArm {
    pub fn name(&self) -> &'static str {
        self.core.name()
    }
}

// ── the Rename seam: what one language answers when a symbol is renamed ─────

/// Where one occurrence of a symbol sits. `span` covers EXACTLY the identifier
/// token: no quotes, no path prefix, no surrounding expression.
pub struct SymbolRef {
    /// Project-relative path of the file that writes the occurrence.
    pub file: String,
    pub span: Span,
    pub role: RefRole,
    /// The bytes at `span` as the arm read them. The core re-reads the tree and
    /// asserts equality before staging; a mismatch is a plan error, not a skip.
    pub text: String,
}

/// What one occurrence does with the symbol. One-for-one with SCIP's
/// `OccurrenceRole` (:1685), so the verify leg compares without a translation.
#[derive(Copy, Clone, Debug, PartialEq, Eq)]
pub enum RefRole {
    Definition,
    /// The imported name in `import {OLD}`.
    Import,
    /// The exported name in `export {OLD}` / `export {x as OLD}`.
    Export,
    Read,
    Write,
    /// A type-position mention; SCIP folds this into READ_ACCESS.
    TypeRef,
}

/// One occurrence a rename reports and never rewrites: where it sits, and the
/// form that reaches the symbol there.
#[derive(Debug)]
pub struct SymbolSeat {
    pub file: String,
    pub span: Span,
    pub form: &'static str,
}

/// Why an arm will not plan. A partial rename compiles less often than no
/// rename at all, so an arm stops instead of emitting a subset.
#[derive(Debug)]
pub enum RenameStop {
    /// `old` names more than one declaration in `anchor`; `at` disambiguates.
    Ambiguous {
        anchor: String,
        old: String,
        sites: Vec<Span>,
    },
    /// `old` names no declaration in `anchor`.
    NotFound { anchor: String, old: String },
    /// A reference the arm found but cannot span exactly.
    Inexact {
        file: String,
        span: Span,
        why: &'static str,
    },
    /// Every reference reachable only through a runtime form (computed member,
    /// dynamic import, string key). One seat at a time hides the next repair.
    Dynamic(Vec<SymbolSeat>),
}

impl fmt::Display for RenameStop {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            RenameStop::Ambiguous { anchor, old, sites } => {
                let offsets: Vec<String> =
                    sites.iter().map(|site| site.start.to_string()).collect();
                write!(
                    formatter,
                    "{anchor} declares {old} more than once, at bytes {}",
                    offsets.join(", ")
                )
            }
            RenameStop::NotFound { anchor, old } => {
                write!(formatter, "{anchor} declares no {old}")
            }
            RenameStop::Inexact { file, span, why } => {
                write!(formatter, "{file} byte {}: {why}", span.start)
            }
            RenameStop::Dynamic(seats) => {
                let lines: Vec<String> = seats
                    .iter()
                    .map(|seat| {
                        format!(
                            "{} byte {}: {} reaches the symbol at runtime",
                            seat.file, seat.span.start, seat.form
                        )
                    })
                    .collect();
                formatter.write_str(&lines.join("\n"))
            }
        }
    }
}

impl std::error::Error for RenameStop {}

/// What one language answers when a symbol it owns is renamed. Sibling to
/// `Rehome`, held `&'static` in the `renames()` roster; no mutable state.
pub trait Rename: Source + Sync + Send {
    /// Every occurrence of `request`'s symbol this language owns, across
    /// `cx.files()`. One parse per file that can reach the anchor.
    fn symbol_refs(
        &self,
        cx: &RenameCx,
        request: &RenameRequest,
    ) -> Result<Vec<SymbolRef>, RenameStop>;

    /// The replacement bytes for one occurrence. None = unchanged (an aliased
    /// import `{OLD as local}` leaves `local` alone).
    fn respell_symbol(
        &self,
        cx: &RenameCx,
        request: &RenameRequest,
        reference: &SymbolRef,
    ) -> Option<Respell>;

    /// Spellings of the old name this language's corpus wears outside the scope
    /// plane, for the `--text-refs` report. NEVER rewritten.
    fn text_spellings(&self, _cx: &RenameCx, _request: &RenameRequest) -> Vec<(String, String)> {
        Vec::new()
    }
}

// ════════════════════════════════════════════════════════════════════════════
// WIRE TYPES  (the flat tagged envelope; serde lives here, NOT on Node<F>)
// ════════════════════════════════════════════════════════════════════════════

/// A span on the wire: inclusive-exclusive byte offsets into the file.
#[derive(Copy, Clone, Debug, PartialEq, Eq, Serialize, serde::Deserialize)]
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
/// A foreign producer spelling this wire is only usable if the rows decode
/// back, so `Deserialize` is as much of the contract as `Serialize`.
#[derive(Serialize, serde::Deserialize, Debug)]
#[serde(tag = "record", rename_all = "lowercase")]
pub enum FlatFact {
    /// The stream's own version. First row of every witnessed stream.
    Protocol {
        version: u32,
    },
    Run(crate::tsi::types::RunOut),
    Fact(crate::tsi::types::FactOut),
    Witness(crate::tsi::types::WitnessOut),
    Coverage(crate::tsi::types::CoverageOut),
    Diagnostic(crate::tsi::types::DiagnosticOut),
    Node {
        #[serde(skip_serializing_if = "Option::is_none", default)]
        fact: Option<u32>,
        family: FamilyTag,
        span: SpanOut,
        kind: String,
        name: Option<String>,
    },
    /// `from_kind`/`to_kind` spell the endpoints' node kinds, so a consumer
    /// keyed on the wire alone carries the whole `(span, kind)` node identity.
    Edge {
        #[serde(skip_serializing_if = "Option::is_none", default)]
        fact: Option<u32>,
        family: FamilyTag,
        kind: String,
        from: SpanOut,
        #[serde(skip_serializing_if = "Option::is_none")]
        from_kind: Option<String>,
        to: SpanOut,
        #[serde(skip_serializing_if = "Option::is_none")]
        to_kind: Option<String>,
    },
    /// DfF parameter slot bridge: one parameter node and its typed-parameter
    /// position. The receiver/self is omitted from the position count.
    #[serde(rename = "param")]
    DfParam {
        #[serde(skip_serializing_if = "Option::is_none", default)]
        fact: Option<u32>,
        family: FamilyTag,
        span: SpanOut,
        pos: u32,
    },
    /// DfF argument slot bridge: one call/new node, signed slot, and argument
    /// node. Method receivers use slot `-1`.
    #[serde(rename = "arg")]
    DfArg {
        #[serde(skip_serializing_if = "Option::is_none", default)]
        fact: Option<u32>,
        family: FamilyTag,
        call: SpanOut,
        pos: i64,
        arg: SpanOut,
    },
    /// DfF named value-into-composite bridge: composite node, field/property/
    /// named-argument name, value node. The pseudo field `..` is a spread /
    /// functional-update base.
    #[serde(rename = "df_field")]
    DfField {
        #[serde(skip_serializing_if = "Option::is_none", default)]
        fact: Option<u32>,
        family: FamilyTag,
        owner: SpanOut,
        name: String,
        value: SpanOut,
    },
    /// DfF string-carrying value node: node, kind lit|template|concat, text
    /// (cooked literal or raw source slice).
    #[serde(rename = "df_lit")]
    DfLit {
        #[serde(skip_serializing_if = "Option::is_none", default)]
        fact: Option<u32>,
        family: FamilyTag,
        node: SpanOut,
        kind: String,
        text: String,
    },
    /// DfF loop: the loop's own span, its variable, and the iterated collection
    /// as written. `var`/`collection` are null where the form names neither.
    #[serde(rename = "df_loop")]
    DfLoop {
        #[serde(skip_serializing_if = "Option::is_none", default)]
        fact: Option<u32>,
        family: FamilyTag,
        span: SpanOut,
        var: Option<String>,
        collection: Option<String>,
    },
    /// DfF loop nest: one call/new node, an enclosing loop, and that loop's rank
    /// in the nest (1 = outermost).
    #[serde(rename = "df_nest")]
    DfNest {
        #[serde(skip_serializing_if = "Option::is_none", default)]
        fact: Option<u32>,
        family: FamilyTag,
        call: SpanOut,
        #[serde(rename = "loop")]
        loop_span: SpanOut,
        depth: u32,
        collection: Option<String>,
    },
    /// DfF allocating callable: the fn/method/closure whose body builds a
    /// collection. Rust only.
    #[serde(rename = "df_allocates")]
    DfAllocates {
        #[serde(skip_serializing_if = "Option::is_none", default)]
        fact: Option<u32>,
        family: FamilyTag,
        owner: SpanOut,
    },
    /// TypeF arrow-type sig: owner = callable span, slot = param/ret, pos, ty.
    Sig {
        #[serde(skip_serializing_if = "Option::is_none", default)]
        fact: Option<u32>,
        family: FamilyTag,
        owner: SpanOut,
        owner_start: u32,
        owner_end: u32,
        slot: String,
        pos: u32,
        ty: String,
    },
    /// CallF call site (phase-1 unresolved): span, callee as written, optional path.
    Site {
        #[serde(skip_serializing_if = "Option::is_none", default)]
        fact: Option<u32>,
        family: FamilyTag,
        span: SpanOut,
        callee: String,
        callee_path: Option<String>,
    },
    /// TypeF const value: owner, optional field path, text, kind = lit|template.
    Const {
        #[serde(skip_serializing_if = "Option::is_none", default)]
        fact: Option<u32>,
        family: FamilyTag,
        owner: SpanOut,
        field: Option<String>,
        text: String,
        kind: String,
    },
    /// TypeF doc block: the owning entity's span, its impl owner when it has
    /// one, and the cleaned text.
    Doc {
        #[serde(skip_serializing_if = "Option::is_none", default)]
        fact: Option<u32>,
        family: FamilyTag,
        owner: SpanOut,
        parent: Option<String>,
        text: String,
    },
    /// TypeF doc tag: one structured tag off the block at `owner`.
    #[serde(rename = "doc_tag")]
    DocTagOut {
        #[serde(skip_serializing_if = "Option::is_none", default)]
        fact: Option<u32>,
        family: FamilyTag,
        owner: SpanOut,
        tag: String,
        arg: Option<String>,
        text: String,
    },
    /// DataF document row: one json/jsonl/yaml/toml document of the file.
    /// `doc` is the whole document as a json VALUE, the column `decode/2` reads.
    #[serde(rename = "data_doc")]
    DataDocOut {
        #[serde(skip_serializing_if = "Option::is_none", default)]
        fact: Option<u32>,
        family: FamilyTag,
        ordinal: u32,
        span: SpanOut,
        format: String,
        doc: serde_json::Value,
    },
    /// DataF value row: one value inside a document, addressed by its dotted
    /// path. `text` is null for objects and arrays.
    #[serde(rename = "data_value")]
    DataValueOut {
        #[serde(skip_serializing_if = "Option::is_none", default)]
        fact: Option<u32>,
        family: FamilyTag,
        ordinal: u32,
        path: String,
        kind: String,
        text: Option<String>,
        span: SpanOut,
    },
    /// TypeF doc structure row: heading, code block, link or image. `target`
    /// and `title` ride link and image rows, `body` a code_block with content.
    #[serde(rename = "doc_node")]
    DocNodeOut {
        #[serde(skip_serializing_if = "Option::is_none", default)]
        fact: Option<u32>,
        family: FamilyTag,
        span: SpanOut,
        kind: String,
        name: String,
        parent: Option<String>,
        #[serde(skip_serializing_if = "Option::is_none", default)]
        target: Option<String>,
        #[serde(skip_serializing_if = "Option::is_none", default)]
        title: Option<String>,
        #[serde(skip_serializing_if = "Option::is_none", default)]
        body: Option<SpanOut>,
    },
    /// CallF module specifier (phase-1, as written): span, bound name, kind.
    /// v6-ONLY rows (no v5 oracle facet) — the parity golden reports them,
    /// never asserts them.
    Specifier {
        #[serde(skip_serializing_if = "Option::is_none", default)]
        fact: Option<u32>,
        family: FamilyTag,
        span: SpanOut,
        name: String,
        kind: String,
        /// The source module as written, null when the language puts the
        /// module in `name` (path-only forms).
        module: Option<String>,
        /// The source module's own name for the binding when it differs from
        /// `name`; null when they agree. v5's `module_binding` imported seat.
        imported: Option<String>,
    },
    /// CallF method owner: the declaration a `method` def node belongs to,
    /// joined to it by `owner`. v6-ONLY, no v5 oracle facet.
    #[serde(rename = "method_owner")]
    MethodOwnerOut {
        #[serde(skip_serializing_if = "Option::is_none", default)]
        fact: Option<u32>,
        family: FamilyTag,
        owner: SpanOut,
        self_type: Option<String>,
        #[serde(rename = "trait")]
        trait_name: Option<String>,
    },
    /// CallF cfg scope: a def guarded by a cfg predicate naming `test`, joined
    /// to its def node by `span`. v6-ONLY, no v5 oracle facet.
    #[serde(rename = "cfg_scope")]
    CfgScopeOut {
        #[serde(skip_serializing_if = "Option::is_none", default)]
        fact: Option<u32>,
        family: FamilyTag,
        span: SpanOut,
        cfg: String,
    },
    /// CallF test-only callee: a name this file calls from cfg-guarded sites
    /// ONLY. No span: it is a per-file set row, not a per-occurrence one.
    #[serde(rename = "test_only_call")]
    TestOnlyCallOut {
        #[serde(skip_serializing_if = "Option::is_none", default)]
        fact: Option<u32>,
        family: FamilyTag,
        callee: String,
        cfg: String,
    },
    /// CallF macro site: the invocation `span` whose expansion minted a
    /// def/site elsewhere in this file, and which arm found it.
    #[serde(rename = "macro_site")]
    MacroSiteOut {
        family: FamilyTag,
        span: SpanOut,
        macro_name: String,
        source: String,
    },
    /// A Prolog term-occurrence reference: a compound in argument position,
    /// tagged goal | head_arg | term_arg. Deliberately exceeds the LSP/SCIP
    /// reference set (a data term in argument position is a reference nowhere
    /// else emits one).
    Reference {
        #[serde(skip_serializing_if = "Option::is_none", default)]
        fact: Option<u32>,
        family: FamilyTag,
        span: SpanOut,
        /// The interned `functor/arity` key, e.g. `relplan/5`.
        functor: String,
        /// goal | head_arg | term_arg
        position: String,
    },
    /// CallF runtime-computed edge marker: `detail` is the source text at
    /// `span`. v6-ONLY, no v5 oracle facet.
    #[serde(rename = "unresolved")]
    Unresolved {
        family: FamilyTag,
        /// The file the site sits in. ABSENT from a per-file run, where the
        /// caller already knows it; a resolve run spans files and must say.
        #[serde(skip_serializing_if = "Option::is_none")]
        path: Option<String>,
        span: SpanOut,
        /// dynamic-import | computed-member-call | spread-call-args |
        /// no_corpus_def | ambiguous
        reason: String,
        detail: String,
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
    /// One cross-function value-flow edge (FlowF), BOTH endpoints content-keyed
    /// because flow crosses files. Flattened by `flatten_flow`.
    #[serde(rename = "flow_edge")]
    FlowEdgeOut {
        family: FamilyTag,
        kind: String,
        from_blob: String,
        from: SpanOut,
        to_blob: String,
        to: SpanOut,
    },
    /// A project-mode CLI call edge. Paths and names are top-level fields so
    /// line-oriented consumers can decode the record without span joins.
    #[serde(rename = "resolved_edge")]
    ResolvedEdge {
        #[serde(skip_serializing_if = "Option::is_none", default)]
        fact: Option<u32>,
        caller_path: String,
        caller_name: Option<String>,
        callee_path: String,
        callee_name: Option<String>,
        caller_site_start: u32,
        caller_site_end: u32,
        kind: String,
        /// Which resolver leg answered (`ResolutionOrigin::as_str`).
        resolution_origin: String,
    },
    /// A project-mode `Resolve<TypeF>` edge: one type reference resolved to the
    /// declaration it names. The flat twin of `ProjectEdge`, for the same reason
    /// as `ResolvedEdge` above: the v6 host decodes top-level keys, so the
    /// target coordinate travels as a path plus a name, never a nested span
    /// join. `owner` is the referencing declaration, `target` what it resolved
    /// to.
    #[serde(rename = "resolved_type_edge")]
    ResolvedTypeEdge {
        #[serde(skip_serializing_if = "Option::is_none", default)]
        fact: Option<u32>,
        owner_path: String,
        owner_name: Option<String>,
        owner_start: u32,
        owner_end: u32,
        target_path: String,
        target_name: Option<String>,
        kind: String,
        /// Which resolver leg answered (`ResolutionOrigin::as_str`).
        resolution_origin: String,
    },
    // ── the `scip` family: v5's scip_* relation shapes ──────────────────────
    //
    // These eight rows ARE v5's `scip_*` relations (repo-root src/rels/scip.rs
    // decls), projected here rather than left as joins over the passthrough
    // rows. The passthrough rows above and these are two answers to different
    // questions and both ship: `--scip-facts` is every field the protobuf
    // carries, unjoined; `--family scip` is the v5 relation vocabulary a
    // program already knows how to read.
    //
    // v5's `scip_occurrence` and `scip_binding` are NOT among them, and the
    // reason is a wire collision, not a gap in the port: `scip_occurrence` is
    // ALREADY a record tag on this wire (the byte-span passthrough row above),
    // with different fields. Two shapes under one tag is the silent-drift
    // hazard every golden here exists to stop. Both v5 rows are one consumer
    // join off `--scip-facts --scip-record scip_occurrence`, which carries the
    // spans and every role bit; `scip_binding`'s source-slice need is answered
    // by that row's optional `text` field under --occurrence-text (issue
    // extract-scip-vocab-occurrence-binding).
    /// v5 `scip_def(symbol, file, repo)`: a symbol's defining document.
    #[serde(rename = "scip_def")]
    ScipDefRow {
        symbol: String,
        file: String,
        repo: String,
    },
    /// v5 `scip_name(symbol, name)`: the trailing identifier run of a moniker.
    /// Computed here because it needs the moniker grammar's `[`/`]`/`#`
    /// separators, which a single-separator string split cannot all honor.
    #[serde(rename = "scip_name")]
    ScipNameRow { symbol: String, name: String },
    /// v5 `scip_ref(file, symbol, def_file, repo)`: a non-definition occurrence
    /// of a symbol this index also defines.
    #[serde(rename = "scip_ref")]
    ScipRefRow {
        file: String,
        symbol: String,
        def_file: String,
        repo: String,
    },
    /// v5 `scip_edge(src, dst, repo)`: file-to-file dependency, one row per
    /// distinct pair. The same graph `--scip-deps` folds, in v5's column names.
    #[serde(rename = "scip_edge")]
    ScipEdgeRow {
        src: String,
        dst: String,
        repo: String,
    },
    /// v5 `scip_fn_edge(caller, callee)`: the function-level call graph, the
    /// caller being the innermost enclosing callable definition.
    #[serde(rename = "scip_fn_edge")]
    ScipFnEdgeRow { caller: String, callee: String },
    /// v5 `scip_callee_type(sym, type)`: the receiver type parsed out of a
    /// method moniker's `impl#[T]` / `for#[T]` segment.
    #[serde(rename = "scip_callee_type")]
    ScipCalleeTypeRow {
        sym: String,
        #[serde(rename = "type")]
        receiver_type: String,
    },
    /// v5 `scip_local(fn, name)`: a local binding or parameter attributed to
    /// its enclosing callable.
    #[serde(rename = "scip_local")]
    ScipLocalRow {
        #[serde(rename = "fn")]
        enclosing_fn: String,
        name: String,
    },
    /// v5 `scip_impl(impl, iface)`: the implements / overrides edge, from a
    /// SymbolInformation relationship with `is_implementation`.
    #[serde(rename = "scip_impl")]
    ScipImplRow {
        #[serde(rename = "impl")]
        implementor: String,
        iface: String,
    },
    /// The `scip` family's index header: which tool answered, and whether an
    /// index already on disk was reused or one was built. Self-diagnosis on the
    /// wire, so a caller never has to ask why a stream is the size it is.
    /// The index PATH is deliberately absent: it is machine-dependent and would
    /// pin a checkout location into every golden. It goes to stderr instead.
    #[serde(rename = "scip_index")]
    ScipIndexRow {
        reused: bool,
        tool_name: String,
        tool_version: String,
        documents: u32,
    },
    /// A NAMED SKIP: one detected indexer produced no index, and why. This is a
    /// row rather than an exit code on purpose. A root with no toolchain must
    /// not kill its caller (v5's law: a missing indexer skips the repo, it never
    /// fails the tick), and it must not produce a silently empty stream either,
    /// which reads as "this project has no symbols". `reason` is the stable
    /// slug to match on, `detail` the human half.
    #[serde(rename = "scip_skip")]
    ScipSkipRow {
        lang: String,
        bin: String,
        reason: String,
        detail: String,
    },
    /// A NAMED SKIP on SIZE, `scip_skip`'s per-file twin: one input was over the
    /// byte ceiling, so it was not parsed. `limit` rides the row: it is a flag.
    #[serde(rename = "size_skip")]
    SizeSkipRow {
        path: String,
        bytes: u64,
        limit: u64,
        reason: String,
    },
    /// One SCIP occurrence: a symbol mentioned at a byte span in one document.
    /// RAW index fact, deliberately unjoined. v5's `scip_def` is this row with
    /// `definition` true, `scip_ref` is it with `definition` false, and
    /// `scip_local` is it with a `local `-prefixed symbol; those splits are one
    /// filter each in the dl layer, which is where the machines live.
    #[serde(rename = "scip_occurrence")]
    ScipOccurrenceRow {
        path: String,
        symbol: String,
        start: u32,
        end: u32,
        /// The raw scip.proto SymbolRole bitfield, kept whole so no role is
        /// lost in projection.
        roles: i32,
        /// The seven SymbolRole bits, one column each. Hoisted for the same
        /// reason `definition` always was: bit arithmetic in a dl rule is
        /// worse than a column, and `roles` alone left six of the seven roles
        /// unreachable from the language.
        definition: bool,
        import: bool,
        write_access: bool,
        read_access: bool,
        generated: bool,
        test: bool,
        forward_definition: bool,
        /// The raw scip.proto SyntaxKind ordinal (0 = unspecified).
        syntax_kind: i32,
        /// The nearest enclosing AST node's byte span, null when the indexer
        /// emitted no enclosing range or the range did not convert.
        enclosing_start: Option<u32>,
        enclosing_end: Option<u32>,
        /// The source slice at the occurrence's byte span, lossy-utf8. Absent
        /// (not null, not empty) unless `--occurrence-text` asked for it and
        /// the span fits the corpus bytes; the answer to v5 scip_binding's
        /// `local_name` (issue extract-scip-vocab-occurrence-binding).
        #[serde(skip_serializing_if = "Option::is_none")]
        text: Option<String>,
    },
    /// One range-specific documentation string on an occurrence
    /// (scip.proto `Occurrence.override_documentation`). `pos` is its index in
    /// the repeated field, so a multi-paragraph doc keeps its order.
    #[serde(rename = "scip_occurrence_doc")]
    ScipOccurrenceDocRow {
        path: String,
        start: u32,
        end: u32,
        pos: u32,
        text: String,
    },
    /// One compiler diagnostic the indexer reported at an occurrence's range
    /// (scip.proto `Occurrence.diagnostics`). `severity` and `tags` are raw
    /// enum ordinals; `tags` is a JSON array because the field is repeated.
    #[serde(rename = "scip_diagnostic")]
    ScipDiagnosticRow {
        path: String,
        start: u32,
        end: u32,
        severity: i32,
        code: String,
        message: String,
        source: String,
        tags: Vec<i32>,
    },
    /// One SCIP symbol information row: v5's `scip_name`. `path` is the
    /// document that declared it, or null for an index's external symbols.
    #[serde(rename = "scip_symbol")]
    ScipSymbolRow {
        path: Option<String>,
        symbol: String,
        display_name: String,
        /// The raw scip.proto SymbolInformation.Kind enum value.
        kind: i32,
        /// The owning symbol of a local symbol; empty for global symbols.
        enclosing_symbol: String,
    },
    /// One markdown docstring entry on a symbol
    /// (scip.proto `SymbolInformation.documentation`). `pos` is its index in
    /// the repeated field.
    #[serde(rename = "scip_documentation")]
    ScipDocumentationRow {
        symbol: String,
        pos: u32,
        text: String,
    },
    /// One rendered type signature (scip.proto
    /// `SymbolInformation.signature_documentation`).
    #[serde(rename = "scip_signature")]
    ScipSignatureRow {
        symbol: String,
        language: String,
        text: String,
    },
    /// One reference inside a signature's text. `start`/`end` are byte offsets
    /// into the SIGNATURE TEXT, never into a document, which is why this is
    /// its own record instead of another `scip_occurrence`.
    #[serde(rename = "scip_signature_occurrence")]
    ScipSignatureOccurrenceRow {
        symbol: String,
        ref_symbol: String,
        start: u32,
        end: u32,
        roles: i32,
    },
    /// One index's metadata (scip.proto `Metadata` + `ToolInfo`), one row per
    /// index. `project_root` is the only place an index states what corpus it
    /// describes, and the tool identity is what a ledger entry needs to say
    /// which indexer release produced a fact.
    #[serde(rename = "scip_metadata")]
    ScipMetadataRow {
        version: i32,
        tool_name: String,
        tool_version: String,
        tool_arguments: Vec<String>,
        project_root: String,
        text_document_encoding: i32,
    },
    /// One indexed document's own header (scip.proto `Document` minus its
    /// repeated children). `text` is null unless the indexer inlined the
    /// document's contents, which it does only for virtual documents.
    #[serde(rename = "scip_document")]
    ScipDocumentRow {
        path: String,
        language: String,
        position_encoding: i32,
        text: Option<String>,
    },
    /// One SCIP relationship between two symbols: v5's `scip_impl` and the
    /// symbol half of `scip_edge`. The four flags are not exclusive; scip.proto
    /// sets several at once for an overriding method.
    #[serde(rename = "scip_relationship")]
    ScipRelationshipRow {
        symbol: String,
        related_symbol: String,
        is_reference: bool,
        is_implementation: bool,
        is_type_definition: bool,
        is_definition: bool,
    },
    /// One file-to-file dependency edge, derived from a SCIP index: `src_path`
    /// contains a non-definition occurrence of a symbol whose definition lives
    /// in `dst_path`. `symbols` is how many distinct symbols cross that edge.
    ///
    /// This is the ONE derived relation the extractor projects rather than
    /// leaving to the dl layer, and the reason is measured, not stylistic: over
    /// v6/tsv2 (212 TypeScript files) the raw occurrence rows are 122,317 and
    /// the edges they fold to are 755. Shipping the occurrences to compute the
    /// edges above the wire is a 160x amplification of a fact one pass over a
    /// hashmap produces here. The raw rows stay available under `--scip-facts`
    /// for every other join.
    ///
    /// It is v5's `module_edge` by another name, and it exists because v6 has no
    /// TypeScript module resolver; SCIP bypasses the resolver entirely.
    ///
    /// `kind` is the `SpecifierKind` slug that bound the crossing, so one
    /// (src, dst) pair carries one row per import form and `symbols` counts the
    /// distinct names of THAT form. `--scip-deps` fills it `unknown`: an index
    /// records resolved occurrences, never the statement that bound the name.
    #[serde(rename = "file_edge")]
    FileEdgeRow {
        src_path: String,
        dst_path: String,
        kind: String,
        symbols: u32,
    },
    /// One specifier `--deps` could not turn into an edge, with the resolution
    /// policy that stopped it. v5 called it `module_unresolved`.
    ///
    /// A stop is a FACT, not an absence: `rxjs` stopping at the node_modules
    /// boundary and `./gone.ts` naming nothing are different answers, and
    /// without this row both read as silence.
    #[serde(rename = "file_unresolved")]
    FileUnresolvedRow {
        src_path: String,
        module: String,
        reason: String,
    },
    /// One workspace-internal manifest-to-manifest dependency edge, keyed on
    /// the two manifest paths rather than package names.
    ///
    /// v5's `crate_edge` (`src/graph/modgraph/rust.rs:468`) was Cargo-only and
    /// keyed on crate names. The path key is the same key `file_edge` uses, so
    /// the two grains join without a name dictionary.
    #[serde(rename = "package_edge")]
    PackageEdgeRow {
        src_manifest: String,
        dst_manifest: String,
        kind: String,
    },
    /// One import binding, resolved through the LANGUAGE'S OWN module plane
    /// (ECMAScript ResolveExport for ts/js). The go and rust arms take the same
    /// row shape when their planes land; neither emits it today. Column meanings
    /// live at `--schema`.
    /// @comment-ok: the cross-language contract a second arm has to honor
    #[serde(rename = "resolved_import")]
    ResolvedImportRow {
        src_path: String,
        name: String,
        local: String,
        target_path: String,
        target_name: Option<String>,
        kind: String,
        hops: u32,
    },
    /// One file, once: its byte length and line count. v5's `file_lines` and
    /// the size half of `content`. `digest` is the same ContentId the phase-2
    /// cache and every resolved edge key on, so this row is what lets a
    /// consumer join a path to the content key without hashing the file again.
    #[serde(rename = "file")]
    FileRow {
        path: String,
        digest: String,
        bytes: u32,
        lines: u32,
    },
}

impl FlatFact {
    /// The row's `fact` ordinal slot, for the arms `flatten_each` numbers. The
    /// envelope rows and the whole-project rows carry none and answer `None`.
    pub fn fact_slot(&mut self) -> Option<&mut Option<u32>> {
        match self {
            FlatFact::Node { fact, .. }
            | FlatFact::Edge { fact, .. }
            | FlatFact::DfParam { fact, .. }
            | FlatFact::DfArg { fact, .. }
            | FlatFact::DfField { fact, .. }
            | FlatFact::DfLit { fact, .. }
            | FlatFact::DfLoop { fact, .. }
            | FlatFact::DfNest { fact, .. }
            | FlatFact::DfAllocates { fact, .. }
            | FlatFact::Sig { fact, .. }
            | FlatFact::Site { fact, .. }
            | FlatFact::Const { fact, .. }
            | FlatFact::Doc { fact, .. }
            | FlatFact::DocTagOut { fact, .. }
            | FlatFact::DataDocOut { fact, .. }
            | FlatFact::DataValueOut { fact, .. }
            | FlatFact::DocNodeOut { fact, .. }
            | FlatFact::Specifier { fact, .. }
            | FlatFact::MethodOwnerOut { fact, .. }
            | FlatFact::CfgScopeOut { fact, .. }
            | FlatFact::TestOnlyCallOut { fact, .. }
            | FlatFact::ResolvedEdge { fact, .. }
            | FlatFact::ResolvedTypeEdge { fact, .. }
            | FlatFact::Reference { fact, .. } => Some(fact),
            _ => None,
        }
    }
}

// flatten / flatten_jsonl live in wire.rs (the logic, not the types).

// ════════════════════════════════════════════════════════════════════════════
// SOURCE-ACTION DRAIN  (drain.rs) - ast-grep edits -> soopy staged mutations
// ════════════════════════════════════════════════════════════════════════════
// @comment-ok: section banner, the same shape as the S1..S6 banners above

/// One ast-grep `Edit` plus what soopy needs and ast-grep never knows: which
/// file the byte offsets index, and which producer emitted them.
pub struct BoundEdit {
    pub source: soopy::ActionSource,
    pub producer: soopy::ActionProducer,
    pub edit: ast_grep_core::source::Edit<String>,
}

/// A `Doc` whose `do_edit` appends a soopy `TextEdit` instead of mutating the
/// string, so one matcher walk collects every edit against ONE frozen parse.
#[derive(Clone)]
pub struct PendingReplaceDoc<L: LanguageExt> {
    src: String,
    lang: L,
    tree: tree_sitter::Tree,
    source: soopy::ActionSource,
    expected: ContentId,
    producer: soopy::ActionProducer,
    edits: Vec<soopy::TextEdit>,
}

impl<L: LanguageExt> PendingReplaceDoc<L> {
    /// Parse once. `expected` is the hash of these exact bytes, so a file that
    /// changes under the walk is refused at stage time, not silently rewritten.
    pub fn open(
        src: &str,
        lang: L,
        source: soopy::ActionSource,
        producer: soopy::ActionProducer,
    ) -> Result<Self, String> {
        let mut parser = tree_sitter::Parser::new();
        parser
            .set_language(&lang.get_ts_language())
            .map_err(|error| error.to_string())?;
        let tree = parser
            .parse(src.as_bytes(), None)
            .ok_or_else(|| "tree-sitter returned no tree".to_string())?;
        Ok(Self {
            src: src.to_string(),
            lang,
            tree,
            source,
            expected: ContentId::blake3(src.as_bytes()),
            producer,
            edits: Vec::new(),
        })
    }

    pub fn lang(&self) -> &L {
        &self.lang
    }

    pub fn tree(&self) -> &tree_sitter::Tree {
        &self.tree
    }

    pub fn source_text(&self) -> &String {
        &self.src
    }

    pub fn expected(&self) -> &ContentId {
        &self.expected
    }

    pub fn edits(&self) -> &[soopy::TextEdit] {
        &self.edits
    }

    pub fn append(&mut self, edit: &ast_grep_core::source::Edit<String>) {
        self.edits.push(
            BoundEdit {
                source: self.source.clone(),
                producer: self.producer.clone(),
                edit: ast_grep_core::source::Edit {
                    position: edit.position,
                    deleted_length: edit.deleted_length,
                    inserted_text: edit.inserted_text.clone(),
                },
            }
            .into(),
        );
    }

    /// None when nothing matched: a Replace with no edits is a staged no-op.
    pub fn into_action(self) -> Option<soopy::SourceAction> {
        if self.edits.is_empty() {
            return None;
        }
        Some(crate::drain::replace_action(
            self.source,
            self.expected,
            self.edits,
        ))
    }
}

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
// AST-GREP LANGUAGE  (lang/extract_lang.rs) - the L in StrDoc<L>
// ════════════════════════════════════════════════════════════════════════════
// @comment-ok: this module mirrors every lang/*.rs shape as a commented sketch
//
// pub enum ExtractLang { Sg(SupportLang), Dl6, Prolog, Markdown, MarkdownInline }
// impl ExtractLang {
//     pub fn from_path(path: &str) -> Option<Self>;  // .dl6/.pl/.md, else SupportLang
//     pub fn name(&self) -> Cow<'static, str>;       // the YAML `language:` spelling
//     pub fn parse_name(name: &str) -> Option<Self>;
// }
// impl Language for ExtractLang      // expando_char '_' for dl6/prolog, 'µ' for md
// impl LanguageExt for ExtractLang   // get_ts_language: the linked LANGUAGE consts
//
// SgRoot = AstGrep<StrDoc<ExtractLang>> (lang/astgrep.rs), so --ast-pattern and
// the YAML rule door reach every grammar in the roster, not just ast-grep's own.

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
//   type_edge (field/impl/variant/uses/generic)   -> TS ASSERTED (4b-iii); GO ASSERTED (4d-i-go, v5 go shape-only: field/impl/generic); rust ASSERTED (4d-i-rust; no sig-sourced rows per v5); kotlin ASSERTED (field/impl/generic/variant)
//   resolved caller -> callee                     -> TS RATCHETED vs scip (4c-ii); GO RATCHETED vs scip-go (4d-ii-go); rust RATCHETED vs rust-analyzer-scip (4d-ii-rust); kotlin DEFERRED to the traits/codegen arc
//   df aux (args/param_pos)                       [x]         [x]            [x]                 [x]
//   df aux (fields)                               [x]         [x]            [x]                 [x]
//   df aux (lits)                                 [x]         [x]            [-] n/a (v5 go emits none)   [-] n/a (v5 kotlin emits none)
//   df aux (loops/nests)                          [x]         [x]            [x]                 [x]
//   df aux (allocates)                            [-] n/a (v5 ts emits none)   [x]   [-] n/a (v5 go emits none)   [-] n/a (v5 kotlin emits none)  // @comment-ok: the status table is one pre-existing prose run
//   inter-procedural flow (FlowF)                 -> landed; the resolve_project dispatch is the follow-up  // @comment-ok: the status table is one pre-existing prose run
//
// LEAF INFRA (pure CPU; still this leaf): parallel dispatch (rayon, arena-per-
//   worker); BlobSource impls + the (ContentId, lang, mask) content-keyed cache.
//
// OUT OF SCOPE (engine, another worktree): store-seam wiring (seed Extract trait
//   is todo!()), datalog fixpoint, reactivity, async-eval.
