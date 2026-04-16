//! `mutations` — coroutine-style write-effect plumbing.
//!
//! # Role
//! Ops that mutate external state (fs/shell/render) emit a
//! `MutationRequest` on an mpsc channel and await approval on a oneshot.
//! A `MutationHandler` drains the channel: `AutoApprove` for daemon/tests,
//! `InteractiveCli` for the CLI, `LspPromptBridge` for the LSP (forwards
//! into the `RunEvent::MutationPrompt` stream).
//!
//! # Ownership + lifecycle
//! The evaluator (or DocSession) owns the `mpsc::Sender<MutationRequest>`
//! that every op sees via `OpCtx.mutations`. A `TaskGuard` on the same
//! struct owns the handler task; on reparse/drop the guard aborts the
//! handler and the channel closes.
//!
//! # Who mutates
//! `await_approval` sends into the mpsc; the handler consumes from the
//! matching receiver. `Store::record_effect` persists the outcome after
//! apply.
//!
//! # Failure modes
//! `await_approval` returns `Err(Cancelled)` on three conditions: the
//! mpsc send fails (receiver gone), the cancellation token fires, or the
//! oneshot sender is dropped without firing. `select! { biased; cancel |
//! rx }` guarantees cancel wins even if the ack is pending.

use std::fmt::Debug;
use std::sync::Arc;

use async_trait::async_trait;
use tokio::sync::{broadcast, mpsc, oneshot};
use tokio_util::sync::CancellationToken;

use crate::_0_types::{ParseSite, RunEvent};
use crate::_5_op::OpCtx;
use crate::_task_guard::TaskGuard;
use crate::store::{EffectOutcome, EffectStatus};

pub enum Approve {
    Yes,
    No,
}

pub struct Cancelled;

pub struct EffectErr {
    pub code: &'static str,
    pub msg:  String,
}

pub struct MutationScope {
    pub reader: Arc<dyn crate::Reader>,
    pub writer: Arc<dyn crate::Writer>,
    pub config: Arc<crate::Config>,
}

pub struct MutationRequest {
    pub effect: Arc<dyn MutationEffect>,
    pub ack:    oneshot::Sender<Approve>,
    pub cancel: CancellationToken,
    pub expr:   Option<Arc<str>>,
    pub site:   Arc<ParseSite>,
}

#[async_trait]
pub trait MutationEffect: Send + Sync + Debug + 'static {
    fn kind_sigil(&self)         -> &'static str;
    fn preview_markdown(&self)   -> String;
    fn fingerprint(&self)        -> Arc<str>;
    fn content_stable_since(&self, _: chrono::DateTime<chrono::Utc>) -> bool { false }
    async fn apply(&self, scope: MutationScope) -> Result<EffectOutcome, EffectErr>;
}

#[async_trait]
pub trait MutationHandler: Send + Sync {
    async fn handle(&self, req: MutationRequest);
}

pub struct AutoApprove;

#[async_trait]
impl MutationHandler for AutoApprove {
    async fn handle(&self, _req: MutationRequest) {
        todo!("Phase 2");
    }
}

pub struct InteractiveCli;

impl InteractiveCli {
    pub fn new() -> Self { Self }
}

#[async_trait]
impl MutationHandler for InteractiveCli {
    async fn handle(&self, _req: MutationRequest) {
        todo!("Phase 2");
    }
}

pub struct LspPromptBridge {
    pub events_tx: broadcast::Sender<RunEvent>,
}

impl LspPromptBridge {
    pub fn new(tx: broadcast::Sender<RunEvent>) -> Self {
        Self { events_tx: tx }
    }
}

#[async_trait]
impl MutationHandler for LspPromptBridge {
    async fn handle(&self, _req: MutationRequest) {
        todo!("Phase 2");
    }
}

pub async fn await_approval(
    _ctx:    &OpCtx,
    _effect: Arc<dyn MutationEffect>,
) -> Result<Approve, Cancelled> {
    todo!("Phase 2");
}

pub fn spawn_handler<H: MutationHandler + 'static>(
    _h:      Arc<H>,
    _rx:     mpsc::Receiver<MutationRequest>,
    _cancel: CancellationToken,
) -> TaskGuard {
    TaskGuard::noop()
}

/// Silence unused-import lint until bodies arrive in Phase 2. These items
/// are part of the Phase 1 surface so they stay referenced.
#[allow(dead_code)]
fn _phase_1_keepalive(_: EffectStatus, _: EffectOutcome) {}
