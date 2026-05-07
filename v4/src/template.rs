//! Shared per-cursor renderer for dsl bodies with universal `${...}`
//! carveouts.
//!
//! Every dsl body (str / sh / glob / re / ast / json …) parses through
//! the host pre-pass at walk-time, producing a `Vec<DslInterp>`:
//!   • `InterpKind::Term { mode, field }` — `${X}` / `${X?}` / `${X.f}`.
//!   • `InterpKind::SubPipe { lowered }`  — `${ <pipe> }` carveout.
//!
//! `render_segments` is the one canonical consumer: it walks the literal
//! body in byte-range order, splices `c.get(key)` for Term interps, and
//! drains the pre-lowered nested `Pipe<Cursor>` for SubPipe interps. The
//! drained value is the focal `value` of the first emitted cursor (or
//! "" if the inner pipe emits zero).
//!
//! Consumers:
//!   • `pipeline::StrTemplateComponent`           — str template body.
//!   • `v2_ops::ShComponent` / `ShBangComponent`  — shell command template.
//!   • `compile/lower/ops::GlobDef`               — slow path: per-cursor
//!     glob body → regex recompile when SubPipe present.
//!   • `v2_ops::ReComponent` / `AstNmComponent` / `JsonComponent` — slow
//!     path: per-cursor pattern body → recompile when SubPipe present.

use std::sync::Arc;

use effect_runtime::v2::{
    expand, Component, Diag, ExpandOpts, MemQueue, Node, Pipe, QueueBackend, RenderCtx,
};

use crate::compile::lower::op_def::{DslInterp, InterpKind};
use crate::Cursor;

/// Per-cursor render of a dsl body. Returns the literal body with each
/// interp's span replaced by the resolved string.
///
/// `interps` must come from `DslBody.interps` — pre-sorted by `range.lo`
/// and pre-walked so SubPipe interps carry `lowered = Some(_)`.
///
/// Term interps that miss a binding emit a `term/unbound-at-interp`
/// diagnostic via `ctx.diag` and splice empty (matches the legacy
/// `StrTemplateComponent` behavior). SubPipe interps that lack a
/// lowered pipe (walk-time gap) emit `subpipe/unlowered` and splice
/// empty.
pub fn render_segments(
    body: &str,
    interps: &[DslInterp],
    outer: &Cursor,
    ctx:   &RenderCtx,
) -> String {
    let mut out = String::with_capacity(body.len());
    let mut head: usize = 0;
    for interp in interps.iter() {
        let lo = interp.range.lo as usize;
        let hi = interp.range.hi as usize;
        if lo > body.len() || hi > body.len() || lo < head { continue; }
        out.push_str(&body[head..lo]);
        match &interp.kind {
            InterpKind::Term { mode: _, field } => {
                let key: std::borrow::Cow<'_, str> = match field {
                    None    => (&*interp.name).into(),
                    Some(f) => format!("{}.{}", interp.name, f).into(),
                };
                match outer.get(&key) {
                    Some(v) => out.push_str(v),
                    None    => ctx.diag.emit(
                        Diag::error(
                            "term/unbound-at-interp",
                            format!("${{{}}} not bound at dsl interp", key),
                        ),
                    ),
                }
            }
            InterpKind::SubPipe { lowered, .. } => {
                if let Some(pipe) = lowered {
                    out.push_str(&drain_subpipe(pipe.clone(), outer));
                } else {
                    ctx.diag.emit(
                        Diag::error(
                            "subpipe/unlowered",
                            "${ pipe } sub-pipe was not lowered (walk-time gap)",
                        ),
                    );
                }
            }
        }
        head = hi;
    }
    out.push_str(&body[head..]);
    out
}

/// Per-cursor render without a `RenderCtx`. Diagnostics are silently
/// dropped (replaced by the empty string for unbound terms / unlowered
/// sub-pipes). Lower-time pattern recompiles use this path because they
/// run inside `par_render` closures with no host diag sink in scope.
pub fn render_segments_no_diag(
    body: &str,
    interps: &[DslInterp],
    outer: &Cursor,
) -> String {
    let mut out = String::with_capacity(body.len());
    let mut head: usize = 0;
    for interp in interps.iter() {
        let lo = interp.range.lo as usize;
        let hi = interp.range.hi as usize;
        if lo > body.len() || hi > body.len() || lo < head { continue; }
        out.push_str(&body[head..lo]);
        match &interp.kind {
            InterpKind::Term { mode: _, field } => {
                let key: std::borrow::Cow<'_, str> = match field {
                    None    => (&*interp.name).into(),
                    Some(f) => format!("{}.{}", interp.name, f).into(),
                };
                if let Some(v) = outer.get(&key) { out.push_str(v); }
            }
            InterpKind::SubPipe { lowered, .. } => {
                if let Some(pipe) = lowered {
                    out.push_str(&drain_subpipe(pipe.clone(), outer));
                }
            }
        }
        head = hi;
    }
    out.push_str(&body[head..]);
    out
}

/// Drain a sub-pipe with `outer` as the seed cursor. Returns the focal
/// `value` of the first emitted cursor (or empty string if the pipe
/// produced none).
///
/// Drains via `effect_runtime::v2::expand` against a fresh `MemQueue`
/// so the outer expand's queue isn't disturbed.
pub fn drain_subpipe(pipe: Pipe<Cursor>, outer: &Cursor) -> String {
    use std::sync::Mutex;

    // Tail capture step. The first cursor emitted past the last user
    // step gets recorded here; we return that and stop.
    struct Capture { sink: Arc<Mutex<Option<String>>> }
    impl Component for Capture {
        type Next = Cursor;
        fn render(&self, _: &RenderCtx, c: &Cursor) -> Node<Cursor> {
            let mut g = self.sink.lock().unwrap();
            if g.is_none() { *g = Some(c.value.as_ref().to_string()); }
            Node::Done
        }
    }

    let sink: Arc<Mutex<Option<String>>> = Arc::new(Mutex::new(None));
    let pipe = pipe.step(Arc::new(Capture { sink: sink.clone() }));
    let inst = pipe.into_instance();
    let q: Arc<dyn QueueBackend<Cursor>> = Arc::new(MemQueue::new());
    expand(&inst, q, vec![Arc::new(outer.clone())], ExpandOpts::default());
    let mut g = sink.lock().unwrap();
    g.take().unwrap_or_default()
}

/// Returns true if any interp in the slice carries a SubPipe carveout.
/// Lower-time pattern compilers use this to choose between the static
/// fast path (compile body once, share) and the dynamic slow path
/// (recompile per cursor against `render_segments` output).
pub fn has_subpipe(interps: &[DslInterp]) -> bool {
    interps.iter().any(|i| matches!(i.kind, InterpKind::SubPipe { .. }))
}
