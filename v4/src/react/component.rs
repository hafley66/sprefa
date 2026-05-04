// Component — the React-shaped Op trait.
//
// `render(&Cursor) -> CursorNode`. Pure function modulo the OpCtx
// (which carries sink-gen tracker, telemetry, interner). All async /
// pause / fan-out is expressed via the returned CursorNode shape.
//
// Components are atomic: a render call either completes or panics; it
// cannot pause mid-render. To pause inside what feels like one logical
// step, the author splits into multiple components linked by a private
// sink (or by Effect's `then` continuation).

use std::sync::Arc;

use crate::Cursor;
use super::node::{CursorNode, PipeHash};

pub trait Component: Send + Sync + 'static {
    /// Stable identity for content-addressed caching. Same shape as
    /// the legacy `Op::ident`. blake3 over component-name + config.
    fn ident(&self) -> [u8; 32];

    /// The render function. Pure; takes a single cursor in, returns
    /// a CursorNode tree out. The driver flattens that tree to N
    /// queue rows.
    fn render(&self, ctx: &RenderCtx, cursor: &Cursor) -> CursorNode;

    /// Optional: declare that the driver may fold up to N runnable
    /// rows sharing (pipe_hash, pc) into one render_batch call. None =
    /// always render one cursor at a time.
    fn batch_size_hint(&self) -> Option<usize> { None }

    /// Optional: render N cursors at once. Default falls back to
    /// per-cursor `render` and concatenates results into Many. Override
    /// when amortization across cursors is meaningful (FactWrite
    /// CommitChunk{rows: N}, regex compile reuse, etc.).
    fn render_batch(&self, ctx: &RenderCtx, cursors: &[Cursor]) -> CursorNode {
        let nodes: Vec<CursorNode> = cursors.iter()
            .map(|c| self.render(ctx, c))
            .collect();
        CursorNode::Many(nodes)
    }
}

/// Bridge from existing SimpleOp trait. Any `SimpleOp` is auto-promoted
/// to `Component` via Many+Emit lowering of its step result.
///
/// Lives here (not in lib.rs) to avoid an orphan-impl. Conflicts with
/// any `impl Component for T where T: SimpleOp` written downstream;
/// everything in v4 should lean on this blanket while the migration is
/// in flight.
impl<T: crate::SimpleOp> Component for T {
    fn ident(&self) -> [u8; 32] {
        // Stable kind+instance hash. Mirrors the v3-style ident for
        // SimpleOps without requiring SimpleOp to expose ident itself.
        let mut hasher = blake3::Hasher::new();
        hasher.update(self.kind().as_bytes());
        hasher.update(self.instance().as_bytes());
        let h = hasher.finalize();
        let mut out = [0u8; 32];
        out.copy_from_slice(h.as_bytes());
        out
    }

    fn render(&self, _ctx: &RenderCtx, cursor: &Cursor) -> CursorNode {
        let outs = self.step(cursor);
        match outs.len() {
            0 => CursorNode::Done,
            1 => CursorNode::Emit(std::sync::Arc::new(outs.into_iter().next().unwrap())),
            _ => CursorNode::Many(
                outs.into_iter()
                    .map(|c| CursorNode::Emit(std::sync::Arc::new(c)))
                    .collect()
            ),
        }
    }
}

/// Render-call context. Holds the runtime services a component may
/// reach into during render. Clone-cheap (Arc inside).
#[derive(Clone)]
pub struct RenderCtx {
    pub interner: Arc<crate::Interner>,
    pub tele:     crate::Telemetry,
    pub pipe:     PipeHash,
    pub pc:       u32,
}
