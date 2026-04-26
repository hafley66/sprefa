//! `ProbeOp` — strict-bound predicate over a relation bag.
//!
//! All-bound `<rel>?(:r, $X, $Y)` lowers here. The materialized row is
//! looked up against the named bag; the cursor passes if a row matches,
//! else dropped.
//!
//! Modes:
//!   * [`ProbeMode::Static`] — one-shot snapshot probe. Today's default.
//!   * [`ProbeMode::Reactive`] — on miss, subscribe under the keyed-waiter
//!     index and park until a matching write arrives or runtime cancels.

use std::sync::Arc;

use futures::stream::{self, BoxStream, StreamExt};

use effect_runtime::{BoxFuture, RtCtx, SubjectRegistry};

use crate::_0_cursor::Cursor;
use crate::_1_op::{per_cursor, Op};
use crate::relation_store::{RelationStore, RelationWake};

use super::{materialize_row, RelationArg};

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ProbeMode {
    /// One-shot snapshot probe. Decides at call time, never re-fires.
    Static,
    /// Reactive probe: also re-fires when a matching row arrives later.
    Reactive,
}

#[derive(Debug)]
pub struct ProbeOp {
    pub name: Arc<str>,
    pub key:  Vec<RelationArg>,
    pub mode: ProbeMode,
}

impl ProbeOp {
    pub fn new(name: Arc<str>, key: Vec<RelationArg>, mode: ProbeMode) -> Self {
        Self { name, key, mode }
    }
}

impl Op for ProbeOp {
    fn name(&self) -> &'static str { "tag" }

    fn pipe<'a>(&'a self, ctx: &'a RtCtx, batch: Arc<[Cursor]>) -> BoxFuture<'a, Arc<[Cursor]>> {
        // Static path: snapshot lookup, drop on miss.
        let store = ctx
            .store::<RelationStore>()
            .expect("RelationStore not registered on RtCtx");
        Box::pin(per_cursor(batch, move |c| {
            let store = store.clone();
            async move {
                let key = materialize_row(&self.key, &c);
                if !store.lookup_by_key(&self.name, &key).is_empty() {
                    vec![c]
                } else {
                    Vec::new()
                }
            }
        }))
    }

    fn pipe_flat_map<'a>(
        &'a self,
        ctx: &'a RtCtx,
        upstream: BoxStream<'a, Arc<[Cursor]>>,
    ) -> BoxStream<'a, Arc<[Cursor]>> {
        match self.mode {
            ProbeMode::Static => Box::pin(
                upstream
                    .map(move |b| self.pipe(ctx, b))
                    .buffer_unordered(crate::_1_op::DEFAULT_PIPE_CONCURRENCY),
            ),
            ProbeMode::Reactive => {
                let store = ctx
                    .store::<RelationStore>()
                    .expect("RelationStore not registered on RtCtx");
                let registry = ctx
                    .store::<SubjectRegistry<RelationWake>>()
                    .expect("SubjectRegistry<RelationWake> not registered on RtCtx");
                let cancel = ctx.root_cancel();
                let name = self.name.clone();
                let key_args = self.key.clone();
                Box::pin(
                    upstream
                        .flat_map(|batch| {
                            let cursors: Vec<Cursor> = batch.iter().cloned().collect();
                            stream::iter(cursors)
                        })
                        .flat_map_unordered(None, move |c| {
                            reactive_probe(
                                store.clone(),
                                registry.clone(),
                                cancel.clone(),
                                name.clone(),
                                key_args.clone(),
                                c,
                            )
                        }),
                )
            }
        }
    }
}

/// Reactive per-cursor stream. Snapshot first; on hit emit once + finish.
/// On miss, subscribe under the row key + park until a matching write or
/// cancel; on wake, emit once + finish.
fn reactive_probe(
    store:    Arc<RelationStore>,
    registry: Arc<SubjectRegistry<RelationWake>>,
    cancel:   effect_runtime::CancellationToken,
    name:     Arc<str>,
    key_args: Vec<RelationArg>,
    cursor:   Cursor,
) -> BoxStream<'static, Arc<[Cursor]>> {
    Box::pin(stream::once(async move {
        let key = materialize_row(&key_args, &cursor);
        let subj_key = registry.fresh_key();
        match store.lookup_or_subscribe(&name, key, subj_key, &registry) {
            None => Some(Arc::<[Cursor]>::from(vec![cursor])),
            Some(fut) => {
                let resolved = tokio::select! {
                    r = fut => r,
                    _ = cancel.cancelled() => Err(effect_runtime::Unsubscribed),
                };
                match resolved {
                    Ok(_row) => Some(Arc::<[Cursor]>::from(vec![cursor])),
                    Err(_) => None,
                }
            }
        }
    })
    .filter_map(|opt| async move { opt }))
}
