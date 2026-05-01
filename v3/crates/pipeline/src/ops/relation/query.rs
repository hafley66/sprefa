//! `QueryOp` — drain bag rows + perpetually subscribe.
//!
//! Per cursor: snapshot rows past `last_idx`; if rows present emit a batch
//! and advance; else atomically subscribe and await the next write or
//! cancel. Each upstream cursor opens its own perpetual stream; merged via
//! `flat_map_unordered`.
//!
//! This is the all-unbound `tag?(:r, ${X?}, ${Y?})` (and future
//! `<rule>?(...)`) shape.

use std::sync::Arc;

use futures::stream::{self, BoxStream, StreamExt};

use effect_runtime::{BoxFuture, CancellationToken, RtCtx, SubjectRegistry};

use crate::_0_cursor::Cursor;
use crate::_1_op::Op;
use crate::relation_store::{RelationStore, RelationWake, Row, SnapshotOrSubscribed};

use super::rows_to_batch;

#[derive(Debug)]
pub struct QueryOp {
    pub name:  Arc<str>,
    pub holes: Arc<[Arc<str>]>,
}

impl QueryOp {
    pub fn new(name: Arc<str>, holes: Arc<[Arc<str>]>) -> Self {
        Self { name, holes }
    }
}

impl Op for QueryOp {
    fn name(&self) -> &'static str { "fact" }

    fn pipe<'a>(&'a self, _ctx: &'a RtCtx, batch: Arc<[Cursor]>) -> BoxFuture<'a, Arc<[Cursor]>> {
        // pipe never called for Query — pipe_flat_map takes over.
        Box::pin(async move { batch })
    }

    fn pipe_flat_map<'a>(
        &'a self,
        ctx: &'a RtCtx,
        upstream: BoxStream<'a, Arc<[Cursor]>>,
    ) -> BoxStream<'a, Arc<[Cursor]>> {
        let store = ctx
            .store::<RelationStore>()
            .expect("RelationStore not registered on RtCtx");
        let registry = ctx
            .store::<SubjectRegistry<RelationWake>>()
            .expect("SubjectRegistry<RelationWake> not registered on RtCtx");
        let cancel = ctx.root_cancel();
        let name = self.name.clone();
        let holes = self.holes.clone();
        Box::pin(
            upstream
                .flat_map(|batch| {
                    let cursors: Vec<Cursor> = batch.iter().cloned().collect();
                    stream::iter(cursors)
                })
                .flat_map_unordered(None, move |c| {
                    subscribe_stream(
                        store.clone(),
                        registry.clone(),
                        cancel.clone(),
                        name.clone(),
                        holes.clone(),
                        c,
                    )
                }),
        )
    }
}

/// Build the per-cursor perpetual stream for a relation query.
fn subscribe_stream(
    store:    Arc<RelationStore>,
    registry: Arc<SubjectRegistry<RelationWake>>,
    cancel:   CancellationToken,
    name:     Arc<str>,
    holes:    Arc<[Arc<str>]>,
    cursor:   Cursor,
) -> BoxStream<'static, Arc<[Cursor]>> {
    struct State {
        store:    Arc<RelationStore>,
        registry: Arc<SubjectRegistry<RelationWake>>,
        cancel:   CancellationToken,
        name:     Arc<str>,
        holes:    Arc<[Arc<str>]>,
        cursor:   Cursor,
        last_idx: usize,
    }
    let state = State { store, registry, cancel, name, holes, cursor, last_idx: 0 };
    Box::pin(stream::unfold(state, |mut s| async move {
        loop {
            if s.cancel.is_cancelled() { return None; }
            let key = s.registry.fresh_key();
            match s.store.snapshot_or_subscribe(&s.name, s.last_idx, key, None, &s.registry) {
                SnapshotOrSubscribed::Rows(rows) => {
                    let n = rows.len();
                    let batch = rows_to_batch(&s.cursor, &s.holes, rows);
                    s.last_idx += n;
                    return Some((batch, s));
                }
                SnapshotOrSubscribed::Subscribed(fut) => {
                    let cancel = s.cancel.clone();
                    let resolved = tokio::select! {
                        r = fut => r,
                        _ = cancel.cancelled() => Err(effect_runtime::Unsubscribed),
                    };
                    match resolved {
                        Ok(row_arc) => {
                            let row: Row = (*row_arc).clone();
                            let batch = rows_to_batch(&s.cursor, &s.holes, vec![row]);
                            s.last_idx += 1;
                            return Some((batch, s));
                        }
                        Err(_) => return None,
                    }
                }
            }
        }
    }))
}
