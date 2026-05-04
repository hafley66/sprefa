//! `Component` — render takes a `&Self::Next`, returns `Node<Self::Next>`.
//!
//! Pipes are `Vec<Arc<dyn Component<Next = N>>>`. Trait-object form
//! pins the associated type, so a pipe is homogeneous in N even though
//! the components themselves are heterogeneous.

use std::sync::Arc;

use super::next::Next;
use super::node::Node;
use super::queue::PipeHash;

pub trait Component: Send + Sync + 'static {
    type Next: Next;

    fn render(&self, ctx: &RenderCtx, c: &Self::Next) -> Node<Self::Next>;
}

/// Render-call context. Carries pipe identity and depth; future phases
/// add `EventBus` / `RtCtx` / `ResponseStore` here as the saga surface
/// stabilizes.
#[derive(Clone)]
pub struct RenderCtx {
    pub pipe: PipeHash,
    pub depth:   u32,
}

impl RenderCtx {
    pub fn new(pipe: PipeHash, depth: u32) -> Self {
        Self { pipe, depth }
    }
}

/// Type alias for the trait-object form a pipe stores. Reduces
/// `Vec<Arc<dyn Component<Next = N>>>` noise at use sites.
pub type DynComponent<N> = Arc<dyn Component<Next = N>>;
