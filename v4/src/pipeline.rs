//! Lowered ops: pure pipeline Components, language-agnostic.
//!
//! Contents here construct `effect_runtime::v2::Component<Next = Cursor>`
//! values from plain Rust inputs. Nothing here knows about:
//!   • DSL bodies / `${X}` interpolation
//!   • CST / source byte ranges
//!   • the `OperatorDef` trait or `Registry`
//!
//! `compile/` (today: `lower/`) wraps these with slot specs, `parse_dsl`,
//! and `lower()` to bridge sprefa source text into pipeline values.

use std::sync::Arc;

use effect_runtime::v2::{Component, Node, Pipe, RenderCtx};

use crate::Cursor;

// ─── str ──────────────────────────────────────────────────────────────────

pub struct StrConstComponent {
    pub literal: Arc<str>,
}

impl Component for StrConstComponent {
    type Next = Cursor;

    fn render(&self, _ctx: &RenderCtx, c: &Cursor) -> Node<Cursor> {
        let mut next = c.clone();
        next.value = self.literal.clone();
        Node::Emit(Arc::new(next))
    }
}

/// Runtime template variant of `str`. Each `${X}` interp is resolved
/// per-cursor against `c.terms` at render time. Unbound terms emit a
/// `term/unbound-at-interp` diag and splice empty.
///
/// Op-shape carveouts (`${ pipe }`, task #10) carry a pre-lowered
/// `Pipe<Cursor>` on the interp; render-time drains that pipe with the
/// outer cursor as seed and uses the first drained cursor's focal `value`
/// as the slot value. Multi-cursor drains: the first emitted cursor wins
/// (a follow-up may add concat / fork semantics).
///
/// `interps` are pre-sorted by `range.lo`; ranges are byte offsets into
/// `raw` covering the full `${IDENT}` span (inclusive of the braces).
pub struct StrTemplateComponent {
    pub raw:     Arc<str>,
    pub interps: Arc<Vec<crate::compile::lower::op_def::DslInterp>>,
}

impl Component for StrTemplateComponent {
    type Next = Cursor;

    fn render(&self, ctx: &RenderCtx, c: &Cursor) -> Node<Cursor> {
        let out = crate::template::render_segments(
            self.raw.as_ref(), &self.interps, c, ctx,
        );
        let mut next = c.clone();
        next.value = Arc::from(out);
        Node::Emit(Arc::new(next))
    }
}

/// Convenience builder: a constant-emitting `Pipe<Cursor>`.
pub fn str_pipe(s: &str) -> Pipe<Cursor> {
    Pipe::new().step(Arc::new(StrConstComponent { literal: Arc::from(s) }))
}

// ─── glob ─────────────────────────────────────────────────────────────────

/// Pure text matcher over `cursor.value`. Substrate cleanup
/// (2026-05-07): the filesystem-walking branch was removed — `fs`
/// owns enumeration and stamps `cursor.value = path`; `glob` filters
/// that stream by matching the pattern against the haystack value.
///
/// Capture-aware (2026-05-07): glob bodies use the universal carveout
/// sigils (`${X?}` Bind, `${X}` Read, `$$${X?}` multi-segment Bind).
/// `GlobDef::lower` translates the body to a Rust regex (with named
/// capture groups) and constructs this Component with the compiled
/// regex. Each named group becomes a cursor.term binding at match time.
///
/// Lowering compiles the regex; this struct never panics at
/// construction. Matching is over the full `cursor.value` string;
/// for path-shaped values, patterns like `**/*.rs` compose as expected
/// against absolute or relative paths.
pub struct GlobComponent {
    /// Anchored regex translated from the glob body. Named capture
    /// groups correspond to `${X?}` / `$$${X?}` interps.
    re: regex::Regex,
    /// Cached named-capture list (None entries for un-named groups,
    /// preserved as-is for index alignment with `re.captures`).
    capture_names: Vec<Option<Arc<str>>>,
    /// Layer 0c.2 — content-derived intern store. When attached, named
    /// captures also stamp coord-space terms via `set_synthetic`.
    store: Option<Arc<crate::store::SprfStore>>,
}

impl GlobComponent {
    /// Construct from a pre-compiled regex. `GlobDef::lower` is the only
    /// caller; it handles glob → regex translation including capture
    /// group naming and `$$$` multi-segment detection.
    pub fn new(re: regex::Regex) -> Self {
        let capture_names: Vec<Option<Arc<str>>> = re
            .capture_names()
            .map(|n| n.map(Arc::<str>::from))
            .collect();
        Self { re, capture_names, store: None }
    }
    pub fn with_sprf_store(mut self, s: Arc<crate::store::SprfStore>) -> Self {
        self.store = Some(s); self
    }
}

impl Component for GlobComponent {
    type Next = Cursor;

    fn render(&self, _ctx: &RenderCtx, c: &Cursor) -> Node<Cursor> {
        let hay: &str = c.value.as_ref();
        if hay.is_empty() { return Node::Done; }
        let Some(caps) = self.re.captures(hay) else { return Node::Done; };

        let mut child = c.clone();
        // Walk the named-capture roster. Group 0 is the full match (no
        // name); groups 1.. carry an Option<&str> name we mirror into
        // `capture_names`.
        for (i, name_opt) in self.capture_names.iter().enumerate() {
            let Some(name) = name_opt else { continue };
            let Some(g) = caps.get(i) else { continue };
            // Legacy raw_terms surface: `${X}` reads land here today.
            child.set(name.as_ref(), g.as_str());
            // Layer 0c.2 — coord-space term. SYNTHETIC because no file
            // span is implied by the path-only match (no content read).
            if let Some(store) = &self.store {
                child.set_synthetic(name.as_ref(), g.as_str(), store);
            }
        }
        Node::Emit(Arc::new(child))
    }
}
