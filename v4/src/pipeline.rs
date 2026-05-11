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
    pub raw: Arc<str>,
    pub interps: Arc<Vec<crate::compile::lower::op_def::DslInterp>>,
}

impl Component for StrTemplateComponent {
    type Next = Cursor;

    fn render(&self, ctx: &RenderCtx, c: &Cursor) -> Node<Cursor> {
        let out = crate::template::render_segments(self.raw.as_ref(), &self.interps, c, ctx);
        let mut next = c.clone();
        next.value = Arc::from(out);
        Node::Emit(Arc::new(next))
    }
}

/// Convenience builder: a constant-emitting `Pipe<Cursor>`.
pub fn str_pipe(s: &str) -> Pipe<Cursor> {
    Pipe::new().step(Arc::new(StrConstComponent {
        literal: Arc::from(s),
    }))
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
        Self {
            re,
            capture_names,
            store: None,
        }
    }
    pub fn with_sprf_store(mut self, s: Arc<crate::store::SprfStore>) -> Self {
        self.store = Some(s);
        self
    }
    pub fn matches_value(&self, value: &str) -> bool {
        self.re.captures(value).is_some()
    }
    pub fn regex(&self) -> regex::Regex {
        self.re.clone()
    }
}

impl Component for GlobComponent {
    type Next = Cursor;

    fn describe(&self) -> Option<&dyn std::any::Any> {
        Some(self)
    }

    fn render(&self, _ctx: &RenderCtx, c: &Cursor) -> Node<Cursor> {
        let hay: &str = c.value.as_ref();
        if hay.is_empty() {
            return Node::Done;
        }
        let Some(caps) = self.re.captures(hay) else {
            return Node::Done;
        };

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

/// Slow-path glob with per-cursor body recompile. Used when the body
/// contains a `${ <pipe> }` SubPipe carveout: the inner pipe drains per
/// cursor, the carveout span is replaced by the drained value, the
/// resulting body is translated body→regex via the same scheme as the
/// static path, and the regex is matched against `cursor.value`.
///
/// Term-only carveouts already route through the static `GlobComponent`
/// because their semantics (named-capture / read) are encoded in the
/// pre-compiled regex. SubPipe is the only trigger that requires
/// per-cursor recompile today.
pub struct GlobDynamicComponent {
    /// Original glob body source (with `${ ... }` spans intact).
    body: Arc<str>,
    /// Pre-walked interps (Term + SubPipe) sorted by `range.lo`.
    interps: Arc<Vec<crate::compile::lower::op_def::DslInterp>>,
    /// Sprf store — passed through to children if needed for set_synthetic.
    store: Option<Arc<crate::store::SprfStore>>,
}

impl GlobDynamicComponent {
    pub fn new(body: Arc<str>, interps: Vec<crate::compile::lower::op_def::DslInterp>) -> Self {
        Self {
            body,
            interps: Arc::new(interps),
            store: None,
        }
    }
    pub fn with_sprf_store(mut self, s: Arc<crate::store::SprfStore>) -> Self {
        self.store = Some(s);
        self
    }
}

impl Component for GlobDynamicComponent {
    type Next = Cursor;

    fn render(&self, _ctx: &RenderCtx, c: &Cursor) -> Node<Cursor> {
        let hay: &str = c.value.as_ref();
        if hay.is_empty() {
            return Node::Done;
        }

        // Build the regex per-cursor. SubPipe spans drain to literals
        // (regex-escaped); Term-Bind spans become named capture groups
        // (same shape as the static path); Term-Read spans resolve via
        // Cursor::get and regex-escape; literal text follows the glob
        // metachar table.
        let (regex_src, capture_names) =
            match build_dynamic_glob_regex(&self.body, &self.interps, c) {
                Ok(t) => t,
                Err(_) => return Node::Done,
            };
        let re = match regex::Regex::new(&regex_src) {
            Ok(r) => r,
            Err(_) => return Node::Done,
        };
        let Some(caps) = re.captures(hay) else {
            return Node::Done;
        };

        let mut child = c.clone();
        // capture_names is the explicit name list we emitted; group i+1
        // in the regex aligns 1:1 with capture_names[i]. (Group 0 is the
        // full match.)
        for (i, name) in capture_names.iter().enumerate() {
            let Some(g) = caps.get(i + 1) else { continue };
            child.set(name.as_ref(), g.as_str());
            if let Some(store) = &self.store {
                child.set_synthetic(name.as_ref(), g.as_str(), store);
            }
        }
        Node::Emit(Arc::new(child))
    }
}

/// Per-cursor body→regex builder for `GlobDynamicComponent`.
///
/// Walks `body` interleaving the literal glob translation with interp
/// resolution. Returns `(regex_src, capture_names)` — same calling
/// convention as the static `glob_body_to_regex` path but with SubPipe
/// segments resolved to literal regex-escaped text.
///
/// Carveout shapes:
///   `${X?}`           — named capture `(?P<X>[^/]*)`
///   `$$${X?}`         — multi-segment `(?P<X>.*)` (3-`$` prefix)
///   `${X}`            — Term Read; resolved via `cur.get("X")`,
///                       regex-escaped into the pattern.
///   `${X.field}`      — same as above (key is `X.field`).
///   `${ <pipe> }`     — drain pipe with `cur` as seed; regex-escape
///                       the drained value into the pattern.
fn build_dynamic_glob_regex(
    body: &str,
    interps: &[crate::compile::lower::op_def::DslInterp],
    cur: &Cursor,
) -> Result<(String, Vec<Arc<str>>), ()> {
    use crate::compile::lower::op_def::{InterpKind, InterpMode};
    use crate::template::drain_subpipe;

    let bytes = body.as_bytes();
    let by_lo: std::collections::BTreeMap<u32, &crate::compile::lower::op_def::DslInterp> =
        interps.iter().map(|i| (i.range.lo, i)).collect();

    let mut out = String::with_capacity(body.len() + 8);
    let mut names: Vec<Arc<str>> = Vec::new();
    let mut i: usize = 0;
    while i < bytes.len() {
        let triple_dollar = i + 3 < bytes.len()
            && bytes[i] == b'$'
            && bytes[i + 1] == b'$'
            && bytes[i + 2] == b'$'
            && bytes[i + 3] == b'{';
        let interp_lo = if triple_dollar {
            (i + 2) as u32
        } else {
            i as u32
        };

        if let Some(interp) = by_lo.get(&interp_lo) {
            match &interp.kind {
                InterpKind::Term { mode, field } => {
                    let key: std::borrow::Cow<'_, str> = match field {
                        None => (&*interp.name).into(),
                        Some(f) => format!("{}.{}", interp.name, f).into(),
                    };
                    match mode {
                        InterpMode::Bind => {
                            if triple_dollar {
                                out.push_str("(?P<");
                                out.push_str(interp.name.as_ref());
                                out.push_str(">.*)");
                            } else {
                                out.push_str("(?P<");
                                out.push_str(interp.name.as_ref());
                                out.push_str(">[^/]*)");
                            }
                            names.push(interp.name.clone());
                        }
                        InterpMode::Read => {
                            if let Some(v) = cur.get(&key) {
                                out.push_str(&regex::escape(v));
                            }
                        }
                    }
                }
                InterpKind::SubPipe { lowered, .. } => {
                    if let Some(p) = lowered {
                        let drained = drain_subpipe(p.clone(), cur);
                        out.push_str(&regex::escape(&drained));
                    }
                }
            }
            i = interp.range.hi as usize;
            continue;
        }

        // Literal glob → regex.
        let b = bytes[i];
        match b {
            b'*' => {
                if i + 1 < bytes.len() && bytes[i + 1] == b'*' {
                    out.push_str(".*");
                    i += 2;
                } else {
                    out.push_str("[^/]*");
                    i += 1;
                }
            }
            b'?' => {
                out.push_str("[^/]");
                i += 1;
            }
            b'[' => {
                let mut j = i + 1;
                while j < bytes.len() && bytes[j] != b']' {
                    j += 1;
                }
                if j >= bytes.len() {
                    return Err(());
                }
                out.push_str(&body[i..=j]);
                i = j + 1;
            }
            b'{' => {
                let mut j = i + 1;
                while j < bytes.len() && bytes[j] != b'}' {
                    j += 1;
                }
                if j >= bytes.len() {
                    return Err(());
                }
                out.push_str("(?:");
                let inner = &body[i + 1..j];
                let parts: Vec<&str> = inner.split(',').collect();
                for (k, part) in parts.iter().enumerate() {
                    if k > 0 {
                        out.push('|');
                    }
                    out.push_str(&regex::escape(part));
                }
                out.push(')');
                i = j + 1;
            }
            _ => {
                out.push_str(&regex::escape(&body[i..i + 1]));
                i += 1;
            }
        }
    }
    out.push('$');
    Ok((out, names))
}
