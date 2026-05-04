// ReactEffect — what `CursorNode::Effect` carries.
//
// Distinct from the existing v4 `Effect` enum (Print only). Renamed so
// the two don't collide while the React surface coexists. Once the
// migration is done, the legacy Effect goes away and this can be
// renamed back to plain `Effect`.

use std::sync::Arc;
use std::time::Duration;

use crate::Cursor;
use super::wake::{ExternalToken, SinkId};

#[derive(Debug, Clone)]
pub enum ReactEffect {
    /// Fire-and-forget print. Lowers to the existing `Hooks.effects`
    /// channel for now; `then` runs with the original cursor.
    Print(String),

    /// Write `rows` to `sink`, commit, advance sink gen so awaiters wake.
    /// Phase 5: in-RAM stub — does not call `Store`. Phase 6 wires the
    /// real `Store::insert_many + commit`.
    CommitChunk {
        sink: SinkId,
        rows: Vec<Arc<Cursor>>,
    },

    /// Park the rendering cursor until `sink` commits past `past_gen`.
    /// Equivalent to returning Suspense{wake: SinkGen{...}} but as an
    /// effect so it can carry a continuation. Response cursor has
    /// `:awaited_gen` set.
    AwaitSink {
        sink:       SinkId,
        key_pfx:    Vec<u8>,
        past_gen:   crate::Gen,
    },

    /// Sleep for `duration`, then re-enqueue with `then`. Response
    /// cursor has `:tick` set to the elapsed wall ns.
    Tick(Duration),

    /// Park until external token signalled.
    External(ExternalToken),
}
