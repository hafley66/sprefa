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
use tokio::io::{AsyncBufRead, AsyncBufReadExt, AsyncWriteExt};
use tokio::sync::{mpsc, oneshot, Mutex};
use tokio_util::sync::CancellationToken;

use crate::_0_types::{ParseSite, RunEvent};
use crate::_5_op::OpCtx;
use crate::_task_guard::TaskGuard;
use crate::store::{EffectStatus, EffectResult, EffectOutcome};

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
    async fn handle(&self, req: MutationRequest) {
        let _ = req.ack.send(Approve::Yes);
    }
}

pub struct InteractiveCli {
    stdin: Mutex<Box<dyn AsyncBufRead + Send + Unpin>>,
}

impl InteractiveCli {
    pub fn new() -> Self {
        let stdin: Box<dyn AsyncBufRead + Send + Unpin> =
            Box::new(tokio::io::BufReader::new(tokio::io::stdin()));
        Self { stdin: Mutex::new(stdin) }
    }

    pub fn with_reader<R: AsyncBufRead + Send + Unpin + 'static>(reader: R) -> Self {
        Self { stdin: Mutex::new(Box::new(reader)) }
    }

    async fn prompt(&self, req: &MutationRequest) -> Approve {
        let mut stdout = tokio::io::stdout();
        let header = format!(
            "\n─── [{}] ─────\n{}\nApply? [y/N]: ",
            req.effect.kind_sigil(),
            req.effect.preview_markdown()
        );
        let _ = stdout.write_all(header.as_bytes()).await;
        let _ = stdout.flush().await;
        let mut buf = String::new();
        let mut rdr = self.stdin.lock().await;
        let _ = rdr.read_line(&mut buf).await;
        if buf.trim().eq_ignore_ascii_case("y") { Approve::Yes } else { Approve::No }
    }
}

impl Default for InteractiveCli {
    fn default() -> Self { Self::new() }
}

#[async_trait]
impl MutationHandler for InteractiveCli {
    async fn handle(&self, req: MutationRequest) {
        let ans = tokio::select! {
            biased;
            _ = req.cancel.cancelled() => return,
            ans = self.prompt(&req)    => ans,
        };
        let _ = req.ack.send(ans);
    }
}

/// LSP-side handler: forward each mutation prompt onto a `RunEvent` mpsc
/// so the language server can turn it into a code-action for the client.
///
/// Uses `mpsc` instead of `broadcast` because `RunEvent::MutationPrompt`
/// carries a non-`Clone` `oneshot::Sender`. The LSP handler is a single
/// subscriber anyway.
pub struct LspPromptBridge {
    pub events_tx: mpsc::Sender<RunEvent>,
}

impl LspPromptBridge {
    pub fn new(tx: mpsc::Sender<RunEvent>) -> Self {
        Self { events_tx: tx }
    }
}

#[async_trait]
impl MutationHandler for LspPromptBridge {
    async fn handle(&self, req: MutationRequest) {
        let _ = self.events_tx.send(RunEvent::MutationPrompt {
            effect: req.effect,
            ack:    req.ack,
        }).await;
    }
}

pub async fn await_approval(
    ctx:    &OpCtx,
    effect: Arc<dyn MutationEffect>,
) -> Result<Approve, Cancelled> {
    let status = ctx.store.effect_status(&*effect).await.unwrap_or(EffectStatus::Emit);
    if matches!(status, EffectStatus::Skip) {
        return Ok(Approve::Yes);
    }

    let (ack_tx, ack_rx) = oneshot::channel();
    let req = MutationRequest {
        effect: effect.clone(),
        ack:    ack_tx,
        cancel: ctx.cancel.clone(),
        expr:   ctx.expr_name.clone(),
        site:   ctx.current_site.clone(),
    };
    if ctx.mutations.send(req).await.is_err() {
        return Err(Cancelled);
    }

    tokio::select! {
        biased;
        _   = ctx.cancel.cancelled() => Err(Cancelled),
        ack = ack_rx                 => ack.map_err(|_| Cancelled),
    }
}

pub fn spawn_handler<H: MutationHandler + 'static>(
    h:      Arc<H>,
    mut rx: mpsc::Receiver<MutationRequest>,
    cancel: CancellationToken,
) -> TaskGuard {
    TaskGuard::spawn(async move {
        loop {
            tokio::select! {
                biased;
                _ = cancel.cancelled() => break,
                msg = rx.recv() => match msg {
                    Some(req) => h.handle(req).await,
                    None      => break,
                }
            }
        }
    })
}

/// Silence unused-import lint until bodies land upstream.
#[allow(dead_code)]
fn _phase_1_keepalive(_: EffectResult) {}
