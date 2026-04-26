//! Op trait. Object-safe for storage in `Pipeline::Op(Arc<dyn Op>)`.
//!
//! Stream-to-stream surface (session 20260426.4):
//!   - `pipe(ctx, batch) -> BoxFuture<Arc<[Cursor]>>` is the per-batch
//!     entry point. Default = identity passthrough. Most ops override
//!     this and run per-cursor work inside via the `per_cursor` helper.
//!   - `pipe_flat_map(ctx, upstream) -> BoxStream<Arc<[Cursor]>>` is
//!     the streaming entry point. Default = mergeMap(64) over upstream
//!     calling `pipe` per emitted batch. Streaming ops (tag read, future
//!     subscriptions) override this; finite ops ride the default.
//!
//! Backpressure stays native: futures::Stream is pull-based,
//! `buffer_unordered(N)` caps in-flight and parks upstream poll when full.
//!
//! Strategy choice: mergeMap(N) by default. switchMap / exhaustMap are
//! top-level driver concerns (LSP rerun cancellation, debounce), not
//! the op layer.
//!
//! PAIN POINT — static `type Slot` punted (unchanged from prior shape):
//!
//! Object-safety still forbids `type Slot` on `Op`. Slot access goes
//! via `c.put_slot::<T>` / `c.get_slot::<T>`. See project_op_owned_cursor_slots
//! for the codegen path that gives slot decls back without breaking
//! object safety.

use std::collections::HashMap;
use std::sync::Arc;

use futures::stream::{self, BoxStream, StreamExt};
use regex::bytes::Regex;

use crate::_0_cursor::Cursor;
use crate::value::ArgSpec;
use effect_runtime::{BoxFuture, RtCtx};

/// Default mergeMap concurrency bound for `pipe_flat_map`. Caps the
/// number of in-flight `pipe` calls per op so upstream backpressure
/// kicks in once N batches are mid-flight.
pub const DEFAULT_PIPE_CONCURRENCY: usize = 64;

pub trait Op: Send + Sync + std::fmt::Debug + 'static {
    /// Stable identifier used for `PathSeg::Op { name, .. }` tagging
    /// and for diagnostics. Framework reads this, ops don't.
    fn name(&self) -> &'static str;

    /// Per-slot expectation. The lowerer consults this slot-by-slot to
    /// validate each arg and reject obvious misuses (atom into an op
    /// slot, etc.).
    fn arg_spec(&self) -> &[ArgSpec] { &[] }

    /// Per-batch transform. Default = identity passthrough. Most ops
    /// override; streaming ops leave the default and override
    /// `pipe_flat_map` instead.
    ///
    /// The op may fan-out (output batch larger than input) or filter
    /// (smaller). Returning an empty batch is fine; downstream just
    /// sees nothing for this emission.
    fn pipe<'a>(
        &'a self,
        _ctx:  &'a RtCtx,
        batch: Arc<[Cursor]>,
    ) -> BoxFuture<'a, Arc<[Cursor]>> {
        Box::pin(async move { batch })
    }

    /// Stream entry point. Default = mergeMap(64) over upstream calling
    /// `pipe` per emitted batch. Override only when the op is non-finite
    /// (tag read perpetually subscribes) or wants finer control over
    /// flatten semantics (switchMap, concatMap).
    fn pipe_flat_map<'a>(
        &'a self,
        ctx: &'a RtCtx,
        upstream: BoxStream<'a, Arc<[Cursor]>>,
    ) -> BoxStream<'a, Arc<[Cursor]>> {
        Box::pin(
            upstream
                .map(move |batch| self.pipe(ctx, batch))
                .buffer_unordered(DEFAULT_PIPE_CONCURRENCY),
        )
    }

    /// Sub-grammar for this op's paren-slot body (§14.5).
    fn language(&self) -> Option<tree_sitter::Language> { None }

    /// Editor highlight queries for the sub-grammar (§14.5).
    fn highlights(&self) -> Option<&'static str> { None }

    // -----------------------------------------------------------------
    // Capability surface (§14.5m)
    // -----------------------------------------------------------------

    fn bound_captures(&self) -> &[Arc<str>] { &[] }

    fn try_raw_regex(&self) -> Option<&Regex> { None }

    fn materialize_with(
        &self,
        _bindings: &HashMap<Arc<str>, Vec<u8>>,
    ) -> Option<Regex> { None }

    fn term_positions(&self) -> &[TermPosition] { &[] }

    /// Append this op's identity to the hasher. Return true if the op is
    /// cacheable. Default = false (skip cache for unknown / side-effecting ops).
    ///
    /// Cacheable ops MUST write `self.name().as_bytes()` first then any config
    /// bytes that distinguish two instances. The runner composes the final
    /// cache key as blake3(input_fingerprint || op_cache_bytes).
    fn cache_key(&self, _h: &mut blake3::Hasher) -> bool { false }
}

/// One capture token's location inside a raw paren-body op. `range` is
/// relative to the body start (i.e. immediately after the opening `(`).
#[derive(Debug, Clone)]
pub struct TermPosition {
    pub name:  Arc<str>,
    pub range: std::ops::Range<usize>,
}

/// Per-cursor helper for ops whose work decomposes per input cursor.
/// Runs `f(c)` for each cursor in `batch` with bounded concurrency, then
/// concatenates the per-cursor outputs into a single output batch.
///
/// The common shape: each op file's old per-cursor `pipe` body now lives
/// inside the closure passed here.
///
/// `f` returns `Vec<Cursor>` so an op can fan-out (multiple matches per
/// input), filter (empty vec), or pass-through (`vec![c]`).
pub async fn per_cursor<F, Fut>(batch: Arc<[Cursor]>, f: F) -> Arc<[Cursor]>
where
    F:   Fn(Cursor) -> Fut + Send + Sync,
    Fut: std::future::Future<Output = Vec<Cursor>> + Send,
{
    let outs: Vec<Vec<Cursor>> = stream::iter(batch.iter().cloned())
        .map(|c| f(c))
        .buffer_unordered(DEFAULT_PIPE_CONCURRENCY)
        .collect()
        .await;
    let total: usize = outs.iter().map(|v| v.len()).sum();
    let mut flat: Vec<Cursor> = Vec::with_capacity(total);
    for v in outs { flat.extend(v); }
    Arc::from(flat)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::ops::{FsOp, RepoOp, RevOp, VoidOp};

    #[test]
    fn default_arg_spec_is_empty() {
        let op = VoidOp;
        assert!(op.arg_spec().is_empty());
    }

    #[test]
    fn repo_rev_declare_any_arg() {
        let repo = RepoOp::from_source("myorg/*").unwrap();
        assert!(matches!(repo.arg_spec(), [ArgSpec::Any]));
        let rev = RevOp::from_source("main").unwrap();
        assert!(matches!(rev.arg_spec(), [ArgSpec::Any]));
    }

    #[test]
    fn fs_declares_pattern_arg() {
        let fs = FsOp::from_source("**/*.rs").unwrap();
        assert!(matches!(fs.arg_spec(), [ArgSpec::Op]));
    }
}
