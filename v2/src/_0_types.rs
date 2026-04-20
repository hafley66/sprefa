//! Root static types. No I/O, no traits, no behavior.

use std::any::{Any, TypeId};
use std::collections::HashMap;
use std::marker::PhantomData;
use std::ops::Range;
use std::path::Path;
use std::sync::Arc;

// ---------------------------------------------------------------------------
// IDs
// ---------------------------------------------------------------------------

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Default)]
pub struct RunId(pub u64);

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub struct OpId(pub u64);

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub struct RowId(pub u64);

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub struct FileId(pub u64);

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub struct StringId(pub u64);

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub struct RefId(pub u64);

// ---------------------------------------------------------------------------
// Severity, Span, FilePath
// ---------------------------------------------------------------------------

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum Severity {
    Error,
    Warn,
    Info,
    Hint,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub struct Span {
    pub start: u32,
    pub end: u32,
}

#[derive(Debug, Clone, PartialEq, Eq, Hash)]
pub struct FilePath(pub Arc<Path>);

// ---------------------------------------------------------------------------
// ParseSite — compile-time stable coordinate
// ---------------------------------------------------------------------------

#[derive(Debug, Clone, PartialEq, Eq, Hash)]
pub struct ParseSite {
    pub file: Arc<Path>,
    pub path: Arc<[ParseSeg]>,
    pub byte_range: Range<usize>,
}

#[derive(Debug, Clone, PartialEq, Eq, Hash)]
pub enum ParseSeg {
    Top { index: u16 },
    BraceChild { index: u16 },
    ParenChild { index: u16 },
    PatternLeaf { key: Arc<str> },
}

// ---------------------------------------------------------------------------
// SprfPath — runtime per-cursor trail
// ---------------------------------------------------------------------------

#[derive(Debug, Clone)]
pub struct SprfPath(pub Arc<[PathSeg]>);

impl Default for SprfPath {
    fn default() -> Self {
        SprfPath(Arc::from(Vec::<PathSeg>::new().into_boxed_slice()))
    }
}

#[derive(Debug, Clone)]
pub enum PathSeg {
    Op {
        name: Arc<str>,
        parse_site: Arc<ParseSite>,
        step: u16,
    },
    Named {
        name: Arc<str>,
        key: Arc<str>,
        parse_site: Arc<ParseSite>,
    },
    ForkArm {
        index: u16,
        parse_site: Arc<ParseSite>,
    },
    SwitchArm {
        pat: Arc<str>,
        parse_site: Arc<ParseSite>,
    },
    LeafArm {
        key: Arc<str>,
        parse_site: Arc<ParseSite>,
    },
    Iter {
        index: u64,
    },
}

// ---------------------------------------------------------------------------
// SlotKey<T> + Slots — typed, type-erased per-cursor payload store
// ---------------------------------------------------------------------------

/// Typed handle to a slot. Zero-sized; the `T` lives in the phantom.
///
/// Uses `PhantomData<fn() -> T>` so the key is invariant in `T` but does not
/// inherit `T`'s auto-traits (the key contains no `T`).
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

/// Typed, type-erased slot store keyed by `TypeId::of::<T>()`.
///
/// Payload is `Arc<dyn Any + Send + Sync>`. Cheap to clone.
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

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Tri {
    Claimed,
    Verified,
    Missing,
}

/// Encodes whether a capture's text value is backed by a byte span in the
/// owning content buffer (object/array sub-documents) or was synthesized
/// (JSON strings — unescaped by walker — primitives, computed values).
///
/// CursorRef keys off this to choose rebase behavior:
///   SpanBacked  → narrow byte_range, content unchanged
///   Synthesized → materialize new content bytes from value
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

#[derive(Debug, Clone)]
pub struct Cursor {
    pub run_id: RunId,
    pub repo: Arc<str>,
    pub rev: Arc<str>,
    pub fs: Option<FilePath>,
    pub captures: HashMap<Arc<str>, Capture>,
    pub fks: HashMap<Arc<str>, RowId>,
    pub path: SprfPath,
    pub evidence: Vec<OpEvidence>,
    pub content: Option<Arc<bytes::Bytes>>,
    /// Runtime byte window into `content`. When `Some`, byte-oriented
    /// downstream ops restrict scan to this range. When `None`, the cursor
    /// addresses the whole file.
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
/// runtime execution ignores it. Append-only, leaf-first, one entry per
/// op the cursor passed through that opted in via `Op::witness`.
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

#[derive(Debug, Clone)]
pub struct RunCtx {
    pub run_id: RunId,
    pub op_id: OpId,
    pub path: SprfPath,
}

// ---------------------------------------------------------------------------
// RunEvent — sole Runner output surface
// ---------------------------------------------------------------------------

pub enum RunEvent {
    Cursor {
        expr_name: Option<Arc<str>>,
        cursor: Cursor,
    },
    ExprDone {
        expr_name: Option<Arc<str>>,
    },
    Diag {
        diag: Box<dyn crate::Diagnostic>,
    },
    MutationPrompt {
        effect: Arc<dyn crate::mutations::MutationEffect>,
        ack: tokio::sync::oneshot::Sender<crate::mutations::Approve>,
    },
    Done,
}

#[derive(Clone)]
pub struct CursorExpr {
    pub name: Option<Arc<str>>,
    pub pipeline: crate::_5_op::Pipeline,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum RewriteKind {
    Insert,
    Replace,
    Delete,
}
