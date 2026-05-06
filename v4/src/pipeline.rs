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
/// `interps` are pre-sorted by `range.lo`; ranges are byte offsets into
/// `raw` covering the full `${IDENT}` span (inclusive of the braces).
pub struct StrTemplateComponent {
    pub raw:     Arc<str>,
    pub interps: Arc<Vec<crate::compile::lower::op_def::DslInterp>>,
}

impl Component for StrTemplateComponent {
    type Next = Cursor;

    fn render(&self, ctx: &RenderCtx, c: &Cursor) -> Node<Cursor> {
        let raw = self.raw.as_ref();
        let mut out = String::with_capacity(raw.len());
        let mut head: usize = 0;
        for interp in self.interps.iter() {
            let lo = interp.range.lo as usize;
            let hi = interp.range.hi as usize;
            if lo > raw.len() || hi > raw.len() || lo < head { continue; }
            out.push_str(&raw[head..lo]);
            match c.get(&interp.name) {
                Some(v) => out.push_str(v),
                None    => ctx.diag.emit(
                    effect_runtime::v2::Diag::error(
                        "term/unbound-at-interp",
                        format!("${{{}}} not bound at str interp", interp.name),
                    ),
                ),
            }
            head = hi;
        }
        out.push_str(&raw[head..]);
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

use std::path::PathBuf;

use effect_runtime::v2::{splice_into_at, QueueBackend, QueueRow};
use globset::{Glob, GlobMatcher};
use ignore::WalkBuilder;

/// Tier-3 source op. Walks `root` recursively (gitignore off, hidden on
/// — same shape as `FsComponent`), filters paths against a globset
/// pattern, emits one child Cursor per match with `FS=path`.
///
/// Pattern is compiled at construction (panic on syntax error to fail
/// fast at lower-time). Matches against the path relative to `root`
/// (e.g. `**/*.rs` against `src/lib.rs`).
pub struct GlobComponent {
    pub root:    PathBuf,
    pub pattern: Arc<str>,
    matcher:     GlobMatcher,
    batch:       usize,
}

impl GlobComponent {
    pub fn new(root: PathBuf, pattern: Arc<str>) -> Self {
        let matcher = Glob::new(&pattern)
            .unwrap_or_else(|e| panic!("glob: invalid pattern {pattern:?}: {e}"))
            .compile_matcher();
        Self { root, pattern, matcher, batch: 1024 }
    }
}

impl Component for GlobComponent {
    type Next = Cursor;

    fn dispatch(
        &self,
        ctx:   &RenderCtx,
        rows:  &[QueueRow<Cursor>],
        queue: &dyn QueueBackend<Cursor>,
    ) {
        let parent = match rows.first() { Some(r) => r, None => return };
        let mut buf: Vec<Node<Cursor>> = Vec::with_capacity(self.batch);
        let mut next_idx: u32 = 0;
        let mut flush = |buf: &mut Vec<Node<Cursor>>, next_idx: &mut u32| {
            if buf.is_empty() { return; }
            let many = Node::Many(std::mem::take(buf));
            let n = splice_into_at(parent, many, ctx.depth + 1, ctx.expand_tick, queue, *next_idx);
            *next_idx += n as u32;
        };

        for entry in WalkBuilder::new(&self.root).hidden(true).git_ignore(false).build() {
            let Ok(e) = entry else { continue };
            if !e.file_type().map(|t| t.is_file()).unwrap_or(false) { continue; }
            let p = e.into_path();
            // Match against path relative to root so patterns like
            // `**/*.rs` behave like ripgrep/fd. Fall back to absolute
            // path string if `strip_prefix` fails.
            let rel = p.strip_prefix(&self.root).unwrap_or(&p);
            if !self.matcher.is_match(rel) { continue; }

            let mut c = Cursor::default();
            c.set("FS", p.display().to_string());
            buf.push(Node::Emit(Arc::new(c)));
            if buf.len() >= self.batch { flush(&mut buf, &mut next_idx); }
        }
        flush(&mut buf, &mut next_idx);
    }
}
