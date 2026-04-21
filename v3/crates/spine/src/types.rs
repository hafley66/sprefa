//! Root static types. No I/O, no traits-with-behavior, no runtime state.
//!
//! Every type here is one of: an identifier newtype, a cursor-shape
//! component, a scalar enum, or a runtime-event enum. Behavior lives in
//! other spine modules (reader/writer/diagnostic/mutations traits) or in
//! the sprefa runtime.

use std::any::{Any, TypeId};
use std::collections::HashMap;
use std::marker::PhantomData;
use std::ops::Range;
use std::path::Path;
use std::sync::Arc;

// ---------------------------------------------------------------------------
// IDs
// ---------------------------------------------------------------------------
//
// Monotonic u64 newtypes. Why not a shared `Id<Tag>` generic: the
// type-checker distinguishes `RunId` from `OpId` by nominal identity,
// and callers rarely mix them. The cost of seven one-field structs is
// zero at runtime and marginal at compile.
//
// Cardinality / allocation:
//   - new allocation: zero (u64 is inline)
//   - copies: free (`Copy`, 8 bytes)
//   - hashes: frequent; every per-cursor HashMap keyed by one of these
//     hits the same u64 hash path.

/// Id of a single pipeline run. Bumps on every fresh invocation of the
/// runner (CLI one-shot, LSP reparse, each tick of a scanner loop).
///
/// **When it changes:** per top-level `Evaluator::run()` call. LSP reparses
/// produce a new `RunId` each time the buffer changes.
/// **Cardinality:** O(reparses) for an LSP session (thousands in a long
/// editing session); O(1) for a CLI run.
/// **Allocated:** at evaluator entry, stamped onto every cursor's
/// `run_id` field.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Default)]
pub struct RunId(pub u64);

/// Id of one op *instance* inside a compiled Pipeline.
///
/// **When it changes:** once per op node in the lowered DAG at
/// `lower_rules` time. Stable across runs that reuse the same compiled
/// plan; re-assigned only when the plan is rebuilt.
/// **Cardinality:** equal to op-call count in a .sprf program. Typically
/// tens to low hundreds for a real project; bounded by source size.
/// **Allocated:** once at lower-phase, embedded in `LoweredOp` and on
/// every `OpEvidence` record a cursor accumulates.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub struct OpId(pub u64);

/// Id of a persisted row in SQLite. Assigned by the Store on insert.
///
/// **When it changes:** per row inserted into `rule_<name>` /
/// `violations_<check>` / `relations` tables.
/// **Cardinality:** millions in a full scan of a 500-repo monorepo. This
/// is the highest-frequency ID in the system.
/// **Allocated:** at `Store::flush_batch` / `Store::insert_relation`. FKs
/// in downstream rules reference these.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub struct RowId(pub u64);

/// Id of a (repo, rev, fs-path) triple — the cache key for a blob scan.
///
/// **When it changes:** once per unique file version the runner has
/// looked at. Stable across reparses of the same blob.
/// **Cardinality:** up to (500 repos × avg 10k files × blob distinct
/// revs). Interned, small u64s only.
/// **Allocated:** at first read by the Reader; reused on every
/// subsequent access to the same blob.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub struct FileId(pub u64);

/// Id of an interned string. Used by `str_intern` + Store to shrink
/// repeated `Arc<str>` payloads (repo names, rev names, capture names).
///
/// **When it changes:** first sighting of a new string.
/// **Cardinality:** bounded by the distinct string set — typically
/// O(thousands) for names, O(millions) for capture values before we
/// switch to byte-range references.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub struct StringId(pub u64);

/// Id of a cross-reference `${rule.$V}` edge in the DAG.
///
/// **When it changes:** per parsed xref occurrence at lower time.
/// **Cardinality:** tens to hundreds per .sprf program.
/// **Allocated:** at `CrossRefOccurrence` construction; carried on the
/// `Capture` produced through a cross-ref to anchor the FK row.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub struct RefId(pub u64);

// ---------------------------------------------------------------------------
// Severity, Span, FilePath
// ---------------------------------------------------------------------------

/// Diagnostic severity. `Error` halts compilation in strict CLI mode;
/// `Warn` / `Info` / `Hint` surface in LSP but don't abort.
///
/// **Cardinality:** fixed 4 variants. Per-diagnostic field; thousands of
/// diagnostics per run in an error-heavy session.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum Severity {
    Error,
    Warn,
    Info,
    Hint,
}

/// Half-open byte range `[start, end)` in a source buffer, packed into
/// 8 bytes. Used by LSP-facing types (`TokenSpan`) where we want a
/// smaller footprint than `Range<usize>`.
///
/// **Why u32 not usize:** source files > 4GB are not a target; pack tighter.
/// **Cardinality:** one per tokenized span surfaced to the LSP client —
/// thousands per hover/diagnostic response.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub struct Span {
    pub start: u32,
    pub end: u32,
}

/// Shared filesystem path. `Arc` because every cursor for a given file
/// carries the same path and we don't want one copy per cursor.
///
/// **Cardinality:** O(distinct files scanned). Heavy cursor fan-out at
/// a single file reuses one `Arc<Path>` across all of them.
/// **Allocation:** once per file first touched; cloned by Arc thereafter.
#[derive(Debug, Clone, PartialEq, Eq, Hash)]
pub struct FilePath(pub Arc<Path>);

// ParseSite + ParseSeg live in the `sprefa_parse` crate — they describe
// *source-code* positions, not runtime state, and should ship with the
// parser. Re-exported here so `crate::types::ParseSite` keeps working.
pub use sprefa_parse::{ParseSite, ParseSeg};

// ---------------------------------------------------------------------------
// SprfPath — runtime per-cursor trail through the pipeline
// ---------------------------------------------------------------------------

/// Runtime breadcrumb trail: which ops/forks/switches this cursor has
/// flowed through. Distinct from `ParseSite` which points at source.
///
/// **Structure:** `Arc<[PathSeg]>` — leaf-first, cheap to clone (Arc),
/// never mutated in-place (cursors produce a new `SprfPath` on each
/// traversal step via `Arc::slice` or re-allocation).
/// **Cardinality:** one per cursor. Depth = number of stages traversed
/// (typically 3-10 for a real pipeline). Width = cursor fan-out
/// (millions in a big scan).
/// **Why Arc:** fork arms clone the parent path then append per-arm
/// segments; shared prefix means the clone is pointer-sized.
/// **Never:** reach into this to make runtime decisions. It's telemetry
/// + persistence grouping. Runtime dispatch uses slots/captures.
#[derive(Debug, Clone)]
pub struct SprfPath(pub Arc<[PathSeg]>);

impl Default for SprfPath {
    fn default() -> Self {
        SprfPath(Arc::from(Vec::<PathSeg>::new().into_boxed_slice()))
    }
}

/// One step in a cursor's runtime trail. Six variants cover every way a
/// cursor can be "inside" something at a given stage.
///
/// **Allocation:** one per cursor per stage traversed. Hot path — tens
/// of millions of instances per full scan. The `Arc<str>` / `Arc<ParseSite>`
/// fields are shared across all cursors flowing through the same stage,
/// so actual allocation is O(distinct stages).
///
/// **Emitted by:** the runner at stage entry (not by ops themselves).
/// Ops don't construct `PathSeg`; the framework owns path tagging.
/// **Consumed by:** persistence (rows grouped by path), LSP path-aware
/// hover, diagnostics that want "this cursor came from fork arm 2 of
/// rule foo".
#[derive(Debug, Clone)]
pub enum PathSeg {
    /// Passed through an op in the main chain. `step` is the zero-based
    /// index of this op inside its rule body.
    Op {
        name: Arc<str>,
        parse_site: Arc<ParseSite>,
        step: u16,
    },
    /// Passed through a named projection (e.g. `classes.$NAME`). `name`
    /// is the rule, `key` is the projected capture.
    Named {
        name: Arc<str>,
        key: Arc<str>,
        parse_site: Arc<ParseSite>,
    },
    /// In fork arm `index`. Appended at fork entry; arms never merge
    /// back, so this seg stays on the cursor until it emits or drops.
    ForkArm {
        index: u16,
        parse_site: Arc<ParseSite>,
    },
    /// In a `switch`'s arm matching pattern `pat`.
    SwitchArm {
        pat: Arc<str>,
        parse_site: Arc<ParseSite>,
    },
    /// In a walker-leaf arm keyed by capture `key`.
    LeafArm {
        key: Arc<str>,
        parse_site: Arc<ParseSite>,
    },
    /// Iteration over a structure (array element, map entry). `index`
    /// disambiguates siblings at the same parent.
    Iter {
        index: u64,
    },
}

// ---------------------------------------------------------------------------
// SlotKey<T> + Slots — typed, type-erased per-cursor payload store
// ---------------------------------------------------------------------------

/// Typed handle to a typed slot entry. Zero-sized; only the `T` type
/// parameter matters — `PhantomData<fn() -> T>` keeps the key invariant
/// in `T` without inheriting `T`'s auto-traits.
///
/// **When to declare one:** an op needs to stash typed state on the
/// cursor that other ops in the same chain can read. Example:
/// `json` parses once, stores the tree via `JSON_TREE: SlotKey<JsonTree>`,
/// downstream `jq` ops read it without re-parsing.
/// **Cardinality:** declared as `pub const` per op that opts in —
/// O(tens) across the whole op set. Each const compiles to nothing.
/// **Never:** stash runtime-only state (Arcs to handlers, cancellation
/// tokens). Those belong on `OpCtx`. Slots are for parsed data that is
/// logically part of the cursor's view of the file.
pub struct SlotKey<T: 'static + Send + Sync> {
    _marker: PhantomData<fn() -> T>,
}

impl<T: 'static + Send + Sync> SlotKey<T> {
    /// Const ctor so ops can declare e.g.
    /// `pub const JSON_TREE: SlotKey<JsonTree> = SlotKey::new();`
    pub const fn new() -> Self {
        Self {
            _marker: PhantomData,
        }
    }
}

impl<T: 'static + Send + Sync> Copy for SlotKey<T> {}
impl<T: 'static + Send + Sync> Clone for SlotKey<T> {
    fn clone(&self) -> Self {
        *self
    }
}

/// Typed, type-erased per-cursor payload store keyed by `TypeId::of::<T>()`.
///
/// **Shape:** `HashMap<TypeId, Arc<dyn Any + Send + Sync>>`. The Arc
/// lets fork arms share the parse tree until one arm mutates.
/// **Cardinality:** per-cursor field; entries are O(distinct op types
/// that stashed state) — typically 0 to 3 per cursor in practice.
/// **Allocation:** HashMap allocates on first insert. Most cursors have
/// empty slots maps. Cursor clones share the Arc entries without
/// copying the values.
/// **Cost:** get/set are one `TypeId` hash + one `Arc::downcast`. The
/// downcast is a pointer compare on the vtable — effectively free.
#[derive(Debug, Default, Clone)]
pub struct Slots {
    map: HashMap<TypeId, Arc<dyn Any + Send + Sync>>,
}

impl Slots {
    pub fn is_empty(&self) -> bool {
        self.map.is_empty()
    }
    pub fn len(&self) -> usize {
        self.map.len()
    }

    /// Insert / replace. Returns the prior value if one existed.
    /// Last-write-wins.
    pub fn set<T: 'static + Send + Sync>(&mut self, _k: SlotKey<T>, v: T) -> Option<Arc<T>> {
        let prior = self.map.insert(TypeId::of::<T>(), Arc::new(v));
        prior.and_then(|a| Arc::downcast::<T>(a).ok())
    }

    /// Insert pre-shared Arc. Common when upstream already holds `Arc<T>`.
    pub fn set_arc<T: 'static + Send + Sync>(
        &mut self,
        _k: SlotKey<T>,
        v: Arc<T>,
    ) -> Option<Arc<T>> {
        let prior = self.map.insert(TypeId::of::<T>(), v);
        prior.and_then(|a| Arc::downcast::<T>(a).ok())
    }

    pub fn get<T: 'static + Send + Sync>(&self, _k: SlotKey<T>) -> Option<Arc<T>> {
        self.map
            .get(&TypeId::of::<T>())
            .and_then(|a| Arc::downcast::<T>(a.clone()).ok())
    }

    pub fn contains<T: 'static + Send + Sync>(&self, _k: SlotKey<T>) -> bool {
        self.map.contains_key(&TypeId::of::<T>())
    }

    pub fn remove<T: 'static + Send + Sync>(&mut self, _k: SlotKey<T>) -> Option<Arc<T>> {
        self.map
            .remove(&TypeId::of::<T>())
            .and_then(|a| Arc::downcast::<T>(a).ok())
    }
}

// ---------------------------------------------------------------------------
// Capture + Cursor
// ---------------------------------------------------------------------------

/// Three-valued "has this been confirmed" state on a capture's scan
/// pointer. `Claimed` = upstream op asserted the pointer; `Verified` =
/// a tag-op confirmed against the relations table; `Missing` = claim
/// was checked and failed.
///
/// **Cardinality:** one per `Capture.verified`. Billions across a full
/// scan (one per capture per cursor per stage).
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Tri {
    Claimed,
    Verified,
    Missing,
}

/// Encodes whether a capture's text value is backed by a byte span in
/// the owning content buffer (object/array sub-documents, direct source
/// slices) or was synthesized (JSON strings — unescaped by walker —
/// primitives, computed values).
///
/// **Why it matters:** `&.$X` rebase consults this to pick a strategy.
///   `SpanBacked`  → narrow `byte_range`, content unchanged (zero-copy).
///   `Synthesized` → materialize new content bytes from `value`.
///
/// **Cardinality:** one per Capture — millions per scan. `SpanBacked`
/// keeps the span reference alive but holds no extra bytes.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum CaptureKind {
    /// Raw bytes at `span` in `cursor.content` are valid source bytes for
    /// this capture (e.g. a JSON object or array sub-document).
    SpanBacked { span: Range<usize> },
    /// Value was synthesized: a JSON string (unescaped), a primitive repr,
    /// or a computed value (filepath, repo name, etc.). Raw span is not
    /// meaningful source bytes for downstream parsers.
    Synthesized,
}

/// A named binding on a cursor. Every `$NAME` capture produced by an op
/// materializes as one `Capture` entry in `cursor.captures`.
///
/// **Fields:**
///   - `value` — the text the capture was bound to (Arc-shared if from a
///     source range).
///   - `kind` — where the value came from (see `CaptureKind`).
///   - `ref_id` — set when this capture came through a cross-rule xref;
///     anchors the FK row in `relations`.
///   - `scan_pointer` — tag-op metadata. `None` for ordinary captures.
///   - `verified` — tri-state for scan-pointer verification.
///
/// **Cardinality:** one per named binding per cursor. A cursor with N
/// captures holds an N-entry HashMap. Hot cursors in ast-grep walkers
/// reach 5-15 captures; most cursors have fewer.
/// **Allocation:** per capture write. The `Arc<str>` value is shared
/// with the content Arc when `kind = SpanBacked`; the struct itself is
/// small (~56 bytes).
#[derive(Debug, Clone)]
pub struct Capture {
    pub value: Arc<str>,
    pub kind: CaptureKind,
    pub ref_id: Option<RefId>,
    pub scan_pointer: Option<Arc<str>>,
    pub verified: Tri,
}

impl Capture {
    /// Default ctor — Synthesized. All existing callers (repo/rev/fs/rule
    /// ops) produce non-rebaseable scalar values; this preserves their
    /// behavior without modification.
    pub fn new(value: Arc<str>) -> Self {
        Self {
            value,
            kind: CaptureKind::Synthesized,
            ref_id: None,
            scan_pointer: None,
            verified: Tri::Claimed,
        }
    }
    /// Span-backed ctor — for JSON object/array sub-documents where the
    /// raw bytes at `span` in the file buffer are valid source bytes.
    pub fn span_backed(value: Arc<str>, span: Range<usize>) -> Self {
        Self {
            value,
            kind: CaptureKind::SpanBacked { span },
            ref_id: None,
            scan_pointer: None,
            verified: Tri::Claimed,
        }
    }
    pub fn with_ref_id(mut self, r: RefId) -> Self {
        self.ref_id = Some(r);
        self
    }
    pub fn with_scan(mut self, p: Arc<str>, v: Tri) -> Self {
        self.scan_pointer = Some(p);
        self.verified = v;
        self
    }
}

/// The flow unit. Every op receives a stream of cursors and emits a
/// stream of cursors. Cursors are closures over file content with
/// accumulated captures, stashed slots, and a path trail.
///
/// **Why a struct, not a trait:** one shape for the whole runtime. Ops
/// communicate with future ops through `captures` / `slots` — never by
/// inventing new flow types.
/// **Cardinality:** the biggest type in the system by instance count.
/// Millions per scan. Cloned whenever a fork duplicates or a rebase
/// narrows.
/// **Clone cost:** mostly pointer bumps (every heavy field is `Arc<_>` or
/// `HashMap<Arc<str>, _>`). `captures` / `fks` clones are O(captures
/// count) — typically 0 to 15.
/// **Never buffer across op boundaries:** per project memory budget
/// (16GB for 500 repos), cursors with content must stream. Ops that
/// need to materialize a Vec of cursors should collect minimal
/// projections, not full cursors.
#[derive(Debug, Clone)]
pub struct Cursor {
    /// The run this cursor belongs to. Every cursor in a single scan
    /// shares one `RunId`; on reparse every cursor gets a fresh one.
    pub run_id: RunId,
    /// Repository identifier (`org/repo` or equivalent). Shared `Arc`
    /// across all cursors in the same repo.
    pub repo: Arc<str>,
    /// Git revision (branch / tag / commit / worktree marker).
    pub rev: Arc<str>,
    /// File within the repo. `None` for cursors that haven't been
    /// narrowed to a file yet (e.g., fresh off `repo()`).
    pub fs: Option<FilePath>,
    /// Named bindings accumulated so far. See `Capture`.
    pub captures: HashMap<Arc<str>, Capture>,
    /// Foreign keys into other rules' persisted rows. Populated by
    /// cross-rule xrefs; one entry per resolved edge.
    pub fks: HashMap<Arc<str>, RowId>,
    /// Runtime trail. Framework-appended at stage entry.
    pub path: SprfPath,
    /// Per-op match records. Framework-appended for LSP-visible ops;
    /// ignored by runtime dispatch.
    pub evidence: Vec<OpEvidence>,
    /// Shared file bytes. `None` when the cursor hasn't been rooted at
    /// a file yet. Arc-cloned across cursors pointing at the same file.
    pub content: Option<Arc<bytes::Bytes>>,
    /// Runtime byte window into `content`. When `Some`, byte-oriented
    /// downstream ops restrict scan to this range. When `None`, the
    /// cursor addresses the whole file.
    pub byte_range: Option<Range<usize>>,
    /// Typed per-cursor payload store (parse trees, etc.). See `Slots`.
    pub slots: Slots,
}

impl Default for Cursor {
    fn default() -> Self {
        Self {
            run_id: RunId::default(),
            repo: Arc::from(""),
            rev: Arc::from(""),
            fs: None,
            captures: HashMap::new(),
            fks: HashMap::new(),
            path: SprfPath::default(),
            evidence: Vec::new(),
            content: None,
            byte_range: None,
            slots: Slots::default(),
        }
    }
}

impl Cursor {
    /// Bytes addressed by this cursor. Returns the `byte_range` slice of
    /// `content` when both are set, the whole `content` when `byte_range`
    /// is `None`, and an empty slice when `content` is absent.
    ///
    /// **Hot path:** every byte-reading op calls this first per the
    /// content-contract. Cost is one slice, zero copies.
    pub fn active_bytes(&self) -> &[u8] {
        match (&self.content, &self.byte_range) {
            (Some(bs), Some(r)) => &bs[r.clone()],
            (Some(bs), None) => &bs[..],
            (None, _) => &[],
        }
    }

    /// Shortcut for `self.slots.get(k)`.
    pub fn get_slot<T: 'static + Send + Sync>(&self, k: SlotKey<T>) -> Option<Arc<T>> {
        self.slots.get(k)
    }

    /// Shortcut for `self.slots.set(k, v)`.
    pub fn set_slot<T: 'static + Send + Sync>(&mut self, k: SlotKey<T>, v: T) -> Option<Arc<T>> {
        self.slots.set(k, v)
    }

    /// Rebase this cursor onto `cap`. Two-path per `CaptureKind`:
    ///
    /// - `SpanBacked`: narrow `byte_range` to the capture's span; content
    ///   is unchanged. Parse trees in slots are cleared because they were
    ///   rooted at the whole-file content, not the sub-range.
    /// - `Synthesized`: materialize new `content` bytes from `cap.value`;
    ///   clear `byte_range` and slots.
    ///
    /// All other cursor fields (repo, rev, fs, captures, path, evidence)
    /// are preserved. Path tagging is handled by the pipeline runner.
    pub fn rebase(&self, cap: &Capture) -> Cursor {
        let mut out = self.clone();
        out.slots = Slots::default();
        match &cap.kind {
            CaptureKind::SpanBacked { span } => {
                out.byte_range = Some(span.clone());
            }
            CaptureKind::Synthesized => {
                out.content = Some(Arc::new(bytes::Bytes::copy_from_slice(
                    cap.value.as_bytes(),
                )));
                out.byte_range = None;
            }
        }
        out
    }
}

/// Per-op match record on a cursor. Framework-appended telemetry for LSP;
/// runtime dispatch ignores it. Append-only, leaf-first, one entry per
/// op the cursor passed through that opted in via `Op::witness`.
///
/// **Cardinality:** per-cursor `Vec<OpEvidence>`, N entries = ops opted
/// into witness that the cursor passed. Most ops don't opt in, so the
/// vec is commonly 0-2 entries.
/// **Allocation:** pushed once per opt-in op. `op_name` is a
/// `&'static str` pointing at the op's const NAME — zero alloc.
/// `matched` / `capture` are Arc-shared with the source.
#[derive(Debug, Clone)]
pub struct OpEvidence {
    pub op_name: &'static str,
    pub parse_site: Arc<ParseSite>,
    pub matched: Arc<str>,
    pub capture: Option<Arc<str>>,
}

// ---------------------------------------------------------------------------
// RunCtx — a slice of runtime identity, attached to diagnostics
// ---------------------------------------------------------------------------

/// A three-field identity triple: which run, which op in that run, what
/// `SprfPath` the cursor producing the diagnostic had. Passed to
/// `Diagnostic::render` through the `RunCtx`-aware Renderer so the
/// diagnostic carries runtime provenance without leaking a whole Cursor.
///
/// **Cardinality:** one per diagnostic emission. Small struct; cloned
/// freely.
#[derive(Debug, Clone)]
pub struct RunCtx {
    pub run_id: RunId,
    pub op_id: OpId,
    pub path: SprfPath,
}

// ---------------------------------------------------------------------------
// RunEvent — sole Runner output surface
// ---------------------------------------------------------------------------

/// The one thing the runner emits. Every downstream consumer (CLI,
/// LSP, tests) is a `Stream<Item = RunEvent>` subscriber.
///
/// **Why one enum not N channels:** ordering matters across events —
/// a `Cursor` followed by `ExprDone` cleanly scopes the cursor to its
/// expression. Separate channels would deadlock on cross-channel
/// barriers or require merging logic downstream.
/// **Cardinality:** hot. Millions of `Cursor` variants per scan; one
/// `ExprDone` per named expression; `Diag` variants bounded by error
/// count; one terminating `Done` per run.
/// **Allocation:** `Box<dyn Diagnostic>` and `Arc<dyn MutationEffect>`
/// are heap; `Cursor` is by value (moved into the variant).
pub enum RunEvent {
    /// A cursor emerged from the pipeline. `expr_name` is `Some` when
    /// the cursor is associated with a named top-level expression.
    Cursor {
        expr_name: Option<Arc<str>>,
        cursor: Cursor,
    },
    /// No more cursors will arrive for this named expression. The LSP
    /// uses this to clear in-flight indicators.
    ExprDone {
        expr_name: Option<Arc<str>>,
    },
    /// A diagnostic surfaced during the run. Rendered on the consumer
    /// side via `Diagnostic::render`.
    Diag {
        diag: Box<dyn crate::diagnostic::Diagnostic>,
    },
    /// An op queued a mutation and is waiting for approval. The handler
    /// side pulls these and replies via the oneshot `ack`. LSP turns
    /// these into code-actions; CLI prompts y/N.
    MutationPrompt {
        effect: Arc<dyn crate::mutations::MutationEffect>,
        ack: tokio::sync::oneshot::Sender<crate::mutations::Approve>,
    },
    /// Terminal event. Consumer closes its output once this arrives.
    Done,
}

// CursorExpr references `Pipeline` which still lives in sprefa::op; it
// moves to spine in phase 2 along with the Op trait.

/// How a mutation effect transforms the source file.
///
/// **Cardinality:** one per rewriting effect. Effects that don't touch
/// source (shell-out, index updates) don't use this.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum RewriteKind {
    Insert,
    Replace,
    Delete,
}
