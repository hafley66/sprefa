//! sprefa v3 language lowering layer — minimal enum surface.
//!
//! This file contains the AST that exists *after* parse + name-resolution
//! and *before* runtime execution. Every variant is load-bearing; each
//! comment explains what breaks if it is removed.
//!
//! Design locks honored:
//!   - 6 concepts: Op, Rule, Pipeline, Cursor, Capture, Scalar
//!   - 4 Pipeline cases: Op, Chain, Group, Fork
//!   - 5 EntityRef cases: Scalar, Op, Pipeline, Rule, Capture
//!   - 6 Cursor fields: path, content, byte_range, slots, captures, parent
//!   - 1 sigil: brace-mandatory carveouts `${...}`
//!   - Arg-mode dispatch: ArgSpec + TermMode + OpAction
//!   - Every expression returns a Cursor or stream of Cursors

use std::any::{Any, TypeId};
use std::collections::HashMap;
use std::ops::Range;
use std::sync::Arc;
use std::time::Duration;

// ---------------------------------------------------------------------------
// Scalar — the value tier
// ---------------------------------------------------------------------------

/// The only values that can appear as literals or binding payloads.
///
/// *Why 6 variants?* The language needs: JSON null/bool/number/string
/// interop, symbolic atoms for tags/kinds, and exact integers for
/// line/column indices. Removing `Atom` breaks `:repo`/`:bound` tags.
/// Removing `Int` breaks exact indexing (f64 can't represent >2^53).
/// `Float` and `Int` are split because sprf uses numbers for both
/// line indices and measurements; conflating them loses precision.
#[derive(Clone, Debug, PartialEq)]
pub enum Scalar {
    Null,
    Bool(bool),
    Int(i64),
    Float(f64),
    String(Arc<str>),
    Atom(Arc<str>),
}

// ---------------------------------------------------------------------------
// Term — every argument shape at lowering time
// ---------------------------------------------------------------------------

/// A term is anything that can sit inside an op argument list, a rule
/// parameter list, or a carveout. The lowering layer has already resolved
/// names to slots / rules / ops, so this is explicitly typed.
///
/// *Why 4 variants?*
/// - `Scalar`: literals ("hello", 42, :atom).
/// - `CaptureRef`: `${NAME}` — the universal variable reference.
/// - `Carveout`: `${expr}` — general expression hole; inner pipeline
///   evaluates to cursors whose active bytes become the term value.
/// - `Annotated`: extension point for Section 14 annotations.
///
/// Cursor-field access (`${REPO}`, `${REV}`, `${FS}`) lowers to
/// `CaptureRef` against reserved synthesized names, not a separate
/// variant. Cross-rule refs are rule calls with `${X?}` arg-mode.
#[derive(Clone, Debug)]
pub enum Term {
    Scalar(Scalar),
    CaptureRef(CaptureSlot),
    Carveout(Box<Pipeline>),
    Annotated { term: Box<Term>, annot: Annotation },
}

// ---------------------------------------------------------------------------
// Capture / Binding — named cursor payload
// ---------------------------------------------------------------------------

/// Stable handle assigned at lower time for every in-scope capture.
/// Using a dense `u32` instead of `Arc<str>` avoids string hashing on
/// every cursor clone and makes `Captures` maps compact.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub struct CaptureSlot(pub u32);

/// A single named binding on a cursor.
///
/// *Why `BindingKind`?* `SpanBacked` means downstream ops can `narrow`
/// instead of `rebase` — parse trees in slots stay valid. `Synthesized`
/// means the value came from computation or a cross-ref and must be
/// materialized into fresh bytes. Without this distinction, `&.$X` on
/// a JSON-string capture would try to narrow into unescaped text.
#[derive(Clone, Debug)]
pub struct Binding {
    pub value: Arc<str>,
    pub kind: BindingKind,
}

#[derive(Clone, Debug)]
pub enum BindingKind {
    SpanBacked { byte_range: Range<usize> },
    Synthesized,
}

/// Named capture map. `CaptureSlot` → `Binding`.
///
/// *Why a newtype?* So we can change the backing map (e.g., to
/// `rustc_hash::FxHashMap` or a small-array optimization) without
/// touching every call site.
#[derive(Clone, Debug)]
pub struct Captures {
    inner: HashMap<CaptureSlot, Binding>,
}

impl Default for Captures {
    fn default() -> Self {
        Self {
            inner: HashMap::new(),
        }
    }
}

impl Captures {
    pub fn get(&self, slot: CaptureSlot) -> Option<&Binding> {
        self.inner.get(&slot)
    }
    pub fn insert(&mut self, slot: CaptureSlot, binding: Binding) -> Option<Binding> {
        self.inner.insert(slot, binding)
    }
}

// ---------------------------------------------------------------------------
// SlotMap — typed op-to-op payload store
// ---------------------------------------------------------------------------

/// Type-erased per-cursor slot store. Ops write typed values;
/// downstream ops read via `TypeId` downcast.
///
/// *Why `dyn Any` here?* Slots carry op-local types (parse trees,
/// AST nodes, decoded JSON). A central enum would grow without bound
/// every time a new op is added — violating "ops own everything".
/// `dyn Any` is the one unavoidable trait object; it lives behind `Arc`
/// so Fork-arm clones are refcount bumps only.
pub struct SlotMap {
    inner: HashMap<TypeId, Arc<dyn Any + Send + Sync>>,
}

impl Clone for SlotMap {
    fn clone(&self) -> Self {
        Self {
            inner: self.inner.clone(),
        }
    }
}

impl std::fmt::Debug for SlotMap {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("SlotMap")
            .field("len", &self.inner.len())
            .finish()
    }
}

impl Default for SlotMap {
    fn default() -> Self {
        Self {
            inner: HashMap::new(),
        }
    }
}

impl SlotMap {
    pub fn put<T: Any + Send + Sync + 'static>(&mut self, v: T) {
        self.inner.insert(TypeId::of::<T>(), Arc::new(v));
    }
    pub fn get<T: Any + Send + Sync + 'static>(&self) -> Option<Arc<T>> {
        self.inner
            .get(&TypeId::of::<T>())
            .cloned()
            .and_then(|a| a.downcast::<T>().ok())
    }
}

// ---------------------------------------------------------------------------
// Cursor — Shape C (the universal flow unit)
// ---------------------------------------------------------------------------

/// The unit that flows between ops. Every expression returns a Cursor
/// or a stream of Cursors.
///
/// *Why 6 fields exactly?* The lock says: path, content, byte_range,
/// slots, captures, parent. `fs`/`repo`/`rev` are *not* struct fields;
/// they live as `SlotMap` entries written by their respective ops.
/// This keeps the struct fixed-size regardless of how many metadata
/// dimensions the language accumulates.
///
/// `parent` enables escape-to-ancestor and provenance tracking without
/// re-parsing. Removing it breaks incremental rebase chains.
#[derive(Clone)]
pub struct Cursor {
    pub path: SprfPath,
    pub content: Arc<[u8]>,
    pub byte_range: Range<usize>,
    pub slots: SlotMap,
    pub captures: Captures,
    pub parent: Option<Arc<Cursor>>,
}

impl std::fmt::Debug for Cursor {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("Cursor")
            .field("path", &self.path)
            .field("byte_range", &self.byte_range)
            .field("slots", &self.slots)
            .field("captures", &self.captures)
            .field("parent", &self.parent.is_some())
            .finish()
    }
}

impl Cursor {
    /// PATH-B content read. Every byte-reading op starts here.
    pub fn active(&self) -> &[u8] {
        &self.content[self.byte_range.clone()]
    }

    /// Narrow the byte window without invalidating slots.
    pub fn narrow(&self, inner: Range<usize>) -> Self {
        let mut next = self.clone();
        next.byte_range = inner;
        next
    }

    /// Replace content and clear slots (lifecycle rule).
    pub fn rebase(&self, new_content: Arc<[u8]>, new_range: Range<usize>) -> Self {
        Cursor {
            path: self.path.clone(),
            content: new_content,
            byte_range: new_range,
            slots: SlotMap::default(),
            captures: self.captures.clone(),
            parent: Some(Arc::new(self.clone())),
        }
    }
}

// ---------------------------------------------------------------------------
// SprfPath — runtime provenance trail
// ---------------------------------------------------------------------------

/// `audit/stage1/todos($TEXT)/match3` desugars to a sequence of `PathSeg`s.
/// Every cursor emission appends one segment.
///
/// *Why `Param` in PathSeg?* Parametric rules produce paths that include
/// argument values: `used_by(MyClass).arm_0.step_1`. When a parameter is
/// unbound the path is *partial*; the runner parks the cursor until all
/// params are ground. Without `Param`, partial paths collapse and the
/// runner can't tell `used_by($A)` from `used_by($B)`.
///
/// *Why `Anon`?* Anonymous pipelines (inline `{ > A; > B; }`) need stable
/// identities for persistence and reactive invalidation. The hash is
/// computed from normalized AST at lower time.
#[derive(Clone, Debug, Default)]
pub struct SprfPath(pub Vec<PathSeg>);

#[derive(Clone, Debug)]
pub enum PathSeg {
    /// Named step: rule name, op name, or anonymous block name.
    Step { name: Arc<str>, index: usize },
    /// Fork arm index.
    ForkArm { index: usize },
    /// Parameter binding embedded in the path.
    Param {
        name: Arc<str>,
        value: Option<Arc<str>>,
    },
    /// Anonymous pipeline synthesized identifier.
    Anon { file: Arc<str>, hash: [u8; 8] },
}

// ---------------------------------------------------------------------------
// Pipeline — composition tree (4 cases, locked)
// ---------------------------------------------------------------------------

/// The runtime-composition enum. Four cases, no more.
///
/// *Why no `Switch` at lowering?* `Switch` is a higher-order control op
/// (Rust-implemented), not a Pipeline variant. It lowers to `OpCall`
/// with a `Pipeline` argument. This keeps the core enum minimal and
/// lets `switch` evolve without changing `Pipeline`.
///
/// *Why `Group`?* Needed for explicit precedence and for higher-order
/// ops that take a single sub-pipeline: `retry(Group(...), 3)`. Without
/// it, `retry(Chain([A,B]), 3)` would retry each step individually.
#[derive(Clone, Debug)]
pub enum Pipeline {
    Op(OpCall),
    Chain(Vec<Pipeline>),
    Group(Box<Pipeline>),
    Fork(Vec<Pipeline>),
}

// ---------------------------------------------------------------------------
// OpCall — resolved op invocation with terms
// ---------------------------------------------------------------------------

/// A single op invocation after name resolution and term lowering.
///
/// *Why `Callee::Rule`?* Parametric rules are callable like ops:
/// `used_by($NAME)` in a chain. At lower time the resolver knows the
/// name maps to a rule declaration, not a built-in op.
///
/// *Why `terms: Vec<Term>` and not `args: Vec<Arg>`?* `ArgSpec` lives on
/// the op *declaration* (registry metadata), not the call site. The call
/// site only has terms. Runtime dispatch pairs call-site terms against
/// the op's declared `ArgSpec`s.
#[derive(Clone, Debug)]
pub struct OpCall {
    pub callee: Callee,
    pub terms: Vec<Term>,
    pub site: ParseSite,
}

#[derive(Clone, Debug)]
pub enum Callee {
    Builtin(Arc<str>),
    Rule(Arc<str>),
}

/// Source coordinate for diagnostics. Kept in lowering so ops can
/// emit diagnostics anchored to the original source without re-parsing.
#[derive(Clone, Debug)]
pub struct ParseSite {
    pub file: Arc<str>,
    pub byte_range: Range<usize>,
}

// ---------------------------------------------------------------------------
// Rule — named Pipeline with parameters
// ---------------------------------------------------------------------------

/// A rule is a named, possibly parametric Pipeline.
///
/// *Why `params`?* Zero params = auto-subscribed. N params = lazy
/// until called. The resolver derives per-param `AcceptsMode` from
/// the rule body and stores it here for call-site checking.
///
/// *Why `schema`?* Even if the runner doesn't use it, the writer and
/// LSP do. Keeping it in lowering means one schema walk at lower time,
/// not re-deriving on every run.
#[derive(Clone, Debug)]
pub struct Rule {
    pub name: Arc<str>,
    pub params: Vec<Param>,
    pub body: Arc<Pipeline>,
    pub schema: RowSchema,
    pub subscribe: SubscribePolicy,
}

#[derive(Clone, Debug)]
pub struct Param {
    pub name: Arc<str>,
    pub slot: CaptureSlot,
    pub mode: AcceptsMode,
}

#[derive(Clone, Debug, Default)]
pub struct RowSchema {
    pub columns: Vec<Column>,
}

#[derive(Clone, Debug)]
pub struct Column {
    pub name: Arc<str>,
    pub kind: ColumnKind,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum ColumnKind {
    Arg,
    Capture,
    Both,
}

// ---------------------------------------------------------------------------
// SubscribePolicy — execution scheduling hint
// ---------------------------------------------------------------------------

/// Cold = re-run per subscriber. Shared = materialize once, join later.
/// Memo = shared + TTL expiry.
///
/// *Why 3 variants?* Cold is the default (pure). Shared is required for
/// named rules (persistence tier). Memo is the extension point for
/// higher-order caching ops (`cache(body, ttl)`).
#[derive(Clone, Debug)]
pub enum SubscribePolicy {
    Cold,
    Shared { store_key: Arc<str> },
    Memo { ttl: Duration },
}

#[derive(Clone, Debug)]
pub enum AssertTrigger {
    OnCheckout,
    OnChange(Vec<Arc<str>>),
    Periodic(Duration),
}

// ---------------------------------------------------------------------------
// EntityRef — resolver output for any identifier
// ---------------------------------------------------------------------------

/// The resolver produces one of these for every bare identifier it sees.
///
/// *Why 5 cases exactly?* The lock says Scalar, Op, Pipeline, Rule,
/// Capture. Removing any case means the resolver has no typed way to
/// report that class of entity.
///
/// *Why `Op(Arc<str>)` instead of `Arc<dyn Operator>`?* Lowering happens
/// before the runtime registry is consulted. The name is the stable key;
/// the registry lookup occurs at load/execution time.
#[derive(Clone, Debug)]
pub enum EntityRef {
    Scalar(Scalar),
    Op(Arc<str>),
    Pipeline(Arc<Pipeline>),
    Rule(Arc<str>),
    Capture(CaptureSlot),
}

// ---------------------------------------------------------------------------
// Arg-mode dispatch types
// ---------------------------------------------------------------------------

/// Per-argument mode constraint, declared by the op author.
///
/// *Why 3 variants?* `BoundOnly` = tag-ops that need a concrete value
/// to write a relation row. `UnboundOnly` = ops that exist to *produce*
/// bindings (e.g., a hypothetical `fresh($X)`). `Either` = the default;
/// the op dispatches at runtime.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum AcceptsMode {
    BoundOnly,
    UnboundOnly,
    Either,
}

/// One argument's metadata in an op signature.
#[derive(Clone, Copy, Debug)]
pub struct ArgSpec {
    pub accepts: AcceptsMode,
}

// ---------------------------------------------------------------------------
// TermMode / OpAction — lowering ↔ runtime boundary
// ---------------------------------------------------------------------------

/// Runtime mode of a term, computed per-cursor at op entry.
///
/// *Not stored in lowering.* This is what `resolve_term(cursor, slot)`
/// returns. The lowering layer only stores `CaptureSlot`; the runtime
/// derives `TermMode` by looking up the cursor's `Captures` map.
#[derive(Clone, Debug)]
pub enum TermMode {
    Bound(Scalar),
    Unbound,
}

/// Runtime action an op may request after mode dispatch.
///
/// *Why in lowering?* Higher-order ops (`retry`, `until`, `when`) embed
/// a `Pipeline` and an `OpAction` that controls how the embedded pipeline
/// is invoked. Keeping the enum here lets the lowering layer validate
/// that higher-order args are well-formed.
#[derive(Clone, Debug)]
pub enum OpAction {
    Emit(Cursor),
    Run(Pipeline),
    Drop,
}

// ---------------------------------------------------------------------------
// Annotation — extension point for Section 14
// ---------------------------------------------------------------------------

/// Term annotation grammar (shape not locked). Parsed into this enum so
/// the grammar can evolve without changing `Term`.
///
/// *Why a separate enum?* Annotations are the escape hatch for mode,
/// kind, link, and persistence directives. Parking them inside `Annotation`
/// means `Term` stays 6 variants forever — new syntax extends `Annotation`
/// instead.
#[derive(Clone, Debug)]
pub enum Annotation {
    Mode(ModeHint),
    Kind(Arc<str>),
    Persist(PersistHint),
    Link { target: Arc<str>, via: Arc<str> },
    User { namespace: Arc<str>, tag: Arc<str> },
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum ModeHint {
    Bound,
    Unbound,
    Either,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum PersistHint {
    Persist,
    Ephemeral,
    Annotate,
}
