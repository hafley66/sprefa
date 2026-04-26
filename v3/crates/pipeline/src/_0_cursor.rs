//! Cursor: the typed payload that flows between ops.
//!
//! Cursor is the unit the pipeline runner fans between `Op::pipe` calls.
//! Every byte-reading op reads `cursor.active()` first (content contract
//! PATH B). Slots carry op-to-op typed state. Captures carry match
//! payloads. SprfPath is framework-owned; ops never touch it.
//!
//! Slot lifecycle rule (contract, not enforced by types):
//!   `rebase(new_content, new_range)` clears slots. Any cached
//!   `ParsedTree` / `FixedString` / op-local memo stored in a slot is
//!   tied to the *pre-rebase* content bytes; changing bytes invalidates
//!   those memos. Captures, path, and evidence survive rebase because
//!   they carry their own span references.

use std::any::{Any, TypeId};
use std::collections::HashMap;
use std::ops::Range;
use std::path::Path;
use std::sync::Arc;

/// Framework-owned trail of how a cursor arrived at this point. Every
/// op emission appends `Op { name, step }`; every fork-arm emission
/// appends `ForkArm { index }`. Leaf-first order per project_v2_path_tagging.
/// `PathSeg` carries `&'static str` (interned op names), which the
/// derive macro cannot serialize/deserialize generically without
/// requiring `'de: 'static`. Hand-roll a wire form via `String`,
/// `Box::leak`-ing back to `&'static str` on deserialize. Op names are
/// bounded by the registered op set, so the leak is a fixed-cost
/// interning step.
#[derive(Clone, Debug)]
pub enum PathSeg {
    Op { name: &'static str, step: usize },
    ForkArm { index: usize },
}

#[derive(serde::Serialize, serde::Deserialize)]
enum PathSegWire {
    Op { name: String, step: usize },
    ForkArm { index: usize },
}

impl serde::Serialize for PathSeg {
    fn serialize<S: serde::Serializer>(&self, ser: S) -> Result<S::Ok, S::Error> {
        let wire = match self {
            PathSeg::Op { name, step } => PathSegWire::Op { name: (*name).to_string(), step: *step },
            PathSeg::ForkArm { index } => PathSegWire::ForkArm { index: *index },
        };
        wire.serialize(ser)
    }
}

impl<'de> serde::Deserialize<'de> for PathSeg {
    fn deserialize<D: serde::Deserializer<'de>>(de: D) -> Result<Self, D::Error> {
        let wire = PathSegWire::deserialize(de)?;
        Ok(match wire {
            PathSegWire::Op { name, step } => PathSeg::Op {
                name: Box::leak(name.into_boxed_str()),
                step,
            },
            PathSegWire::ForkArm { index } => PathSeg::ForkArm { index },
        })
    }
}

pub type SprfPath = Vec<PathSeg>;

/// A captured span. `kind` mirrors v2/_0_types.rs:212-264 — SpanBacked
/// means the capture's bytes live at `cursor.content[byte_range]`;
/// Synthesized means the bytes live in `kind.value` and `byte_range` is
/// only a hint (typically `0..0`). Synthesized captures are written by
/// ops like `repo($R)` / `rev($V)` that bind a cursor field (which is
/// not inside `cursor.content`) into a name.
#[derive(Clone, Debug, serde::Serialize, serde::Deserialize)]
pub struct Capture {
    pub name: Arc<str>,
    pub byte_range: Range<usize>,
    pub kind: CaptureKind,
}

#[derive(Clone, Debug, serde::Serialize, serde::Deserialize)]
pub enum CaptureKind {
    /// Bytes live at `cursor.content[byte_range]`. The default shape for
    /// regex/glob/ast captures that name a span inside the file content.
    SpanBacked,
    /// Bytes are carried inline. Produced by ops that bind external
    /// state (repo / rev / fs slug) or by rules that emit a computed
    /// value that has no span in any source file.
    Synthesized { value: Arc<str> },
}

impl Capture {
    /// New SpanBacked capture. The common constructor; used by
    /// capture_write, Pattern::apply, and any op emitting a
    /// content-backed span.
    pub fn span_backed(name: Arc<str>, byte_range: Range<usize>) -> Self {
        Capture { name, byte_range, kind: CaptureKind::SpanBacked }
    }

    /// New Synthesized capture carrying literal bytes. Used by
    /// repo/rev/fs bind mode.
    pub fn synthesized(name: Arc<str>, value: Arc<str>) -> Self {
        Capture { name, byte_range: 0..0, kind: CaptureKind::Synthesized { value } }
    }

    /// Bytes this capture carries, from whichever source its `kind`
    /// selects. Callers resolving bindings (Pattern read mode, hover,
    /// downstream ops) should prefer this over reading
    /// `cursor.content[byte_range]` directly.
    pub fn bytes<'a>(&'a self, cursor_content: &'a [u8]) -> &'a [u8] {
        match &self.kind {
            CaptureKind::SpanBacked => &cursor_content[self.byte_range.clone()],
            CaptureKind::Synthesized { value } => value.as_bytes(),
        }
    }
}

/// The unit flowing between ops.
///
/// `slots` is `Arc<dyn Any + Send + Sync>` so Fork distribution clones
/// are cheap. Downcast happens inside `get_slot::<T>()`; authors see
/// typed readout only.
///
/// `last_bound` records the most recent capture name written by the
/// upstream op. Scan-pointer ops and other annotate-by-reference callers
/// read it to resolve the implicit binding without the source author
/// naming it. Cleared by `rebase`.
#[derive(Clone, serde::Serialize, serde::Deserialize)]
pub struct Cursor {
    pub content: Arc<[u8]>,
    pub byte_range: Range<usize>,
    pub captures: Vec<Capture>,
    pub path: SprfPath,
    pub last_bound: Option<Arc<str>>,
    /// Repo slug the cursor is scoped to. Empty string means "no repo
    /// scope" and is the ctor default. repo/rev/fs are v2-native Cursor
    /// fields (parse.md §14.5c); repo/rev ops read or bind them and fs
    /// uses all three as its walk anchor.
    pub repo: Arc<str>,
    /// Revision (tag/branch/SHA) within `repo`. Same empty-as-none
    /// convention as `repo`.
    pub rev: Arc<str>,
    /// Repo-relative file path the cursor addresses, when known. `None`
    /// before any `fs` op has fanned out.
    #[serde(with = "fs_path_serde")]
    pub fs: Option<Arc<Path>>,
    /// blake3 of the content bytes loaded for this cursor. `Some` once
    /// `ensure_content_loaded` (or a bulk read) has populated `content`
    /// from the reader; `None` for synthetic / pre-read cursors.
    /// Phase 0 cache lock: the leaf `input_hash` for downstream stages
    /// folds blake3(repo || rev || path || content_hash) (sprefa-upb).
    pub content_hash: Option<Arc<[u8; 32]>>,
    /// Slots carry typed payloads (`Arc<dyn Any + Send + Sync>`) which
    /// are not generically serializable. Skipped for L2 disk cache;
    /// downstream ops that depend on producer slots must mark the
    /// producer uncacheable. Documented sharp edge per sprefa-efx.
    #[serde(skip)]
    slots: HashMap<TypeId, Arc<dyn Any + Send + Sync>>,
}

mod fs_path_serde {
    use std::path::{Path, PathBuf};
    use std::sync::Arc;
    use serde::{Deserialize, Deserializer, Serializer};
    pub fn serialize<S: Serializer>(
        v: &Option<Arc<Path>>, ser: S,
    ) -> Result<S::Ok, S::Error> {
        match v {
            Some(p) => ser.serialize_some(&p.to_path_buf()),
            None => ser.serialize_none(),
        }
    }
    pub fn deserialize<'de, D: Deserializer<'de>>(
        de: D,
    ) -> Result<Option<Arc<Path>>, D::Error> {
        let opt: Option<PathBuf> = Option::deserialize(de)?;
        Ok(opt.map(|p| Arc::<Path>::from(p)))
    }
}

impl Default for Cursor {
    fn default() -> Self {
        Cursor {
            content: Arc::from(&[][..]),
            byte_range: 0..0,
            captures: Vec::new(),
            path: Vec::new(),
            last_bound: None,
            repo: Arc::from(""),
            rev: Arc::from(""),
            fs: None,
            content_hash: None,
            slots: HashMap::new(),
        }
    }
}

impl Cursor {
    pub fn new(content: Arc<[u8]>) -> Self {
        let len = content.len();
        Cursor {
            content,
            byte_range: 0..len,
            ..Self::default()
        }
    }

    /// PATH-B content read. Every byte-reading op starts here.
    pub fn active(&self) -> &[u8] {
        &self.content[self.byte_range.clone()]
    }

    /// Stash a typed value under its TypeId. Overwrites prior value of
    /// the same type.
    pub fn put_slot<T: Any + Send + Sync + 'static>(&mut self, v: T) {
        self.slots.insert(TypeId::of::<T>(), Arc::new(v));
    }

    /// Typed readout. None if upstream never populated it, or if a
    /// rebase cleared it.
    pub fn get_slot<T: Any + Send + Sync + 'static>(&self) -> Option<Arc<T>> {
        self.slots
            .get(&TypeId::of::<T>())
            .cloned()
            .and_then(|a| a.downcast::<T>().ok())
    }

    /// Narrow byte_range inside the same content. Slots preserved:
    /// narrowing does not invalidate memos tied to the underlying bytes
    /// (e.g., a ParsedTree of the full file stays valid for any
    /// sub-range).
    pub fn narrow(&self, inner: Range<usize>) -> Cursor {
        let mut next = self.clone();
        next.byte_range = inner;
        next
    }

    /// Replace content. Slots cleared per lifecycle rule. Captures and
    /// path preserved. `last_bound` is cleared because the prior binding
    /// referenced spans in the previous content.
    pub fn rebase(&self, new_content: Arc<[u8]>, new_range: Range<usize>) -> Cursor {
        Cursor {
            content: new_content,
            byte_range: new_range,
            captures: self.captures.clone(),
            path: self.path.clone(),
            last_bound: None,
            repo: self.repo.clone(),
            rev: self.rev.clone(),
            fs: self.fs.clone(),
            // Content swap invalidates the prior hash. The reader-side
            // path that triggers a rebase (`ensure_content_loaded`) sets
            // the new hash from the just-read bytes; pure narrowings that
            // don't change the underlying content use `narrow` and keep
            // their hash via the Clone in that path.
            content_hash: None,
            slots: HashMap::new(),
        }
    }

    /// Look up a capture by name. Returns the most recent binding if
    /// duplicates exist (later writes shadow earlier ones).
    pub fn capture(&self, name: &str) -> Option<&Capture> {
        self.captures.iter().rev().find(|c| &*c.name == name)
    }

    /// Resolve the most recent binding. Returns the capture named by
    /// `last_bound`, or None if no upstream op wrote one. Scan-pointer
    /// ops and similar annotate-by-reference callers use this to pick
    /// up the previous op's named span.
    pub fn last_bound_capture(&self) -> Option<&Capture> {
        self.last_bound
            .as_deref()
            .and_then(|n| self.capture(n))
    }
}
