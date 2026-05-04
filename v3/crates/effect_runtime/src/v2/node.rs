//! `Node<N>` — the discriminated union returned from `Component::render`.
//!
//! Generic over the carrier. Phase-3 variant set: Done, Emit, Many,
//! Suspense. Mount + Effect deferred — they slot in the same place when
//! Phase 5/6 lands.

use std::sync::Arc;

use super::next::Next;
use super::wake::Wake;

#[derive(Debug)]
pub enum Node<N: Next> {
    /// Consume value, emit nothing. Like `null` in JSX.
    Done,

    /// Forward the value to the next pc, immediately runnable.
    Emit(Arc<N>),

    /// Fragment of K children. All flow downstream concurrently; no
    /// implicit ordering between siblings.
    Many(Vec<Node<N>>),

    /// Park the value at pc+1 with a wake condition. The same
    /// component never re-renders the same value; when the wake fires,
    /// the parked row becomes runnable at pc+1.
    Suspense {
        value: Arc<N>,
        wake:  Wake,
    },
}
