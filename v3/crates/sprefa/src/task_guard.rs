//! `TaskGuard` — RAII wrapper around a `tokio::spawn` handle.
//!
//! # Role
//! Bounds the lifetime of every spawned task to the lifetime of the guard.
//! Dropping the guard aborts the handle, so no detached tasks can outlive
//! the evaluator / DocSession / CLI binary that started them.
//!
//! # Ownership + lifecycle
//! Every `tokio::spawn` site in this crate wraps its returned `JoinHandle`
//! in a `TaskGuard`. The guard lives on the spawn site's owning struct or
//! stack frame. When that owner drops — reparse cancels, evaluator drops,
//! CLI exits — the guard's `Drop` impl fires `handle.abort()` before the
//! runtime would have otherwise reaped the task.
//!
//! # Who mutates
//! Immutable after `spawn()`. `join()` and `abort()` consume by value;
//! the inner `JoinHandle` is moved out and the guard is dropped in the
//! same expression. `Drop` reads the `Option` but does not mutate.
//!
//! # Failure modes
//! `abort()` is best-effort: tokio will deliver a cancellation at the
//! next `.await`. A task stuck in a blocking call without await points
//! will run to completion. All known callers cooperate via
//! `CancellationToken`-aware selects, so abort + cancel is the belt-and-
//! suspenders pattern.

use std::future::Future;
use tokio::task::{JoinError, JoinHandle};

pub struct TaskGuard(Option<JoinHandle<()>>);

impl TaskGuard {
    pub fn spawn<F>(fut: F) -> Self
    where
        F: Future<Output = ()> + Send + 'static,
    {
        Self(Some(tokio::spawn(fut)))
    }

    /// Placeholder guard that holds no handle. `Drop` is a no-op.
    pub fn noop() -> Self {
        Self(None)
    }

    pub async fn join(mut self) -> Result<(), JoinError> {
        match self.0.take() {
            Some(h) => h.await,
            None    => Ok(()),
        }
    }

    pub fn abort(mut self) {
        if let Some(h) = self.0.take() {
            h.abort();
        }
    }
}

impl Drop for TaskGuard {
    fn drop(&mut self) {
        if let Some(h) = self.0.take() {
            h.abort();
        }
    }
}
