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
use std::sync::Arc;

/// Framework-owned trail of how a cursor arrived at this point. Every
/// op emission appends `Op { name, step }`; every fork-arm emission
/// appends `ForkArm { index }`. Leaf-first order per project_v2_path_tagging.
#[derive(Clone, Debug)]
pub enum PathSeg {
    Op { name: &'static str, step: usize },
    ForkArm { index: usize },
}

pub type SprfPath = Vec<PathSeg>;

/// A captured span. Minimal shape for this slice: only SpanBacked.
/// CaptureKind trait (SpanBacked vs Synthesized vs …) is a follow-up
/// slice; per v2/_0_types.rs:212-264.
#[derive(Clone, Debug)]
pub struct Capture {
    pub name: Arc<str>,
    pub byte_range: Range<usize>,
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
#[derive(Clone)]
pub struct Cursor {
    pub content: Arc<[u8]>,
    pub byte_range: Range<usize>,
    pub captures: Vec<Capture>,
    pub path: SprfPath,
    pub last_bound: Option<Arc<str>>,
    slots: HashMap<TypeId, Arc<dyn Any + Send + Sync>>,
}

impl Default for Cursor {
    fn default() -> Self {
        Cursor {
            content: Arc::from(&[][..]),
            byte_range: 0..0,
            captures: Vec::new(),
            path: Vec::new(),
            last_bound: None,
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
