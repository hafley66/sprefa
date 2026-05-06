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

/// Convenience builder: a constant-emitting `Pipe<Cursor>`.
pub fn str_pipe(s: &str) -> Pipe<Cursor> {
    Pipe::new().step(Arc::new(StrConstComponent { literal: Arc::from(s) }))
}
