//! `Node<N>` — the discriminated union returned from `Component::render`.
//!
//! Generic over the carrier. Variant set: Done, Emit, Many, Yield.
//! Mount + Effect deferred — they slot in the same place when Phase 5/6
//! lands.

use std::sync::Arc;

use super::next::Next;
use super::wake::Wake;

#[derive(Debug)]
pub enum Node<N: Next> {
    /// Consume value, emit nothing. Like `null` in JSX.
    Done,

    /// Forward the value to the next depth, immediately runnable.
    Emit(Arc<N>),

    /// Fragment of K children. All flow downstream concurrently; no
    /// implicit ordering between siblings.
    Many(Vec<Node<N>>),

    /// Park the value at the SAME depth with a wake condition. When the
    /// wake fires, the same component renders again with the same
    /// value. Models react-query's "pending → success → return data"
    /// pattern: the Component decides what to do based on cache state,
    /// not based on position. Rendering must be idempotent against the
    /// input — the component sees its own input twice.
    Yield { value: Arc<N>, wake: Wake },
}

// Manual Clone impl: `derive(Clone)` would needlessly demand `N: Clone`
// even though every variant only stores `Arc<N>`.
impl<N: Next> Clone for Node<N> {
    fn clone(&self) -> Self {
        match self {
            Node::Done => Node::Done,
            Node::Emit(v) => Node::Emit(v.clone()),
            Node::Many(c) => Node::Many(c.clone()),
            Node::Yield { value, wake } => Node::Yield {
                value: value.clone(),
                wake: wake.clone(),
            },
        }
    }
}
