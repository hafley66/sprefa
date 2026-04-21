//! sprefa::mutations — handler side of the deferred-effect pipeline.
//!
//! The trait surface (`MutationEffect`, `MutationRequest`, `MutationScope`,
//! `Approve`, `Cancelled`, `EffectErr`, `MutationHandler`) lives in the
//! `spine` crate so ops can construct effects without a sprefa dependency.
//!
//! This module keeps the concrete handlers (`AutoApprove`,
//! `InteractiveCli`, `LspPromptBridge`) and the dispatcher logic
//! (`await_approval`, `spawn_handler`) — these need `OpCtx` and the
//! concrete sqlite `Store` to check effect-cache status, so they stay
//! on the sprefa side of the cut.

use std::sync::Arc;

use async_trait::async_trait;
use tokio::io::{AsyncBufRead, AsyncBufReadExt, AsyncWriteExt};
use tokio::sync::{mpsc, oneshot, Mutex};
use tokio_util::sync::CancellationToken;
use tracing::{info_span, Instrument};

use crate::types::RunEvent;
use crate::op::OpCtx;
use crate::task_guard::TaskGuard;
use crate::store::{EffectStatus, EffectResult};

pub use spine::mutations::{
    MutationEffect, MutationRequest, MutationScope,
    Approve, Cancelled, EffectErr, MutationHandler,
};

pub struct AutoApprove;

#[async_trait]
impl MutationHandler for AutoApprove {
    async fn handle(&self, req: MutationRequest) {
        let _s = info_span!("mutations.auto_approve.handle", kind = req.effect.kind_sigil()).entered();
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
        let span = info_span!("mutations.interactive_cli.handle", kind = req.effect.kind_sigil());
        async move {
            let ans = tokio::select! {
                biased;
                _ = req.cancel.cancelled() => return,
                ans = self.prompt(&req)    => ans,
            };
            let _ = req.ack.send(ans);
        }.instrument(span).await
    }
}

/// LSP-side handler: forward each mutation prompt onto a `RunEvent` mpsc
/// so the language server can turn it into a code-action for the client.
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
        let span = info_span!("mutations.lsp_bridge.handle", kind = req.effect.kind_sigil());
        async move {
            let _ = self.events_tx.send(RunEvent::MutationPrompt {
                effect: req.effect,
                ack:    req.ack,
            }).await;
        }.instrument(span).await
    }
}

pub async fn await_approval(
    ctx:    &OpCtx,
    effect: Arc<dyn MutationEffect>,
) -> Result<Approve, Cancelled> {
    let span = info_span!("mutations.await_approval", kind = effect.kind_sigil());
    async move {
    let status = ctx.store.effect_status(&*effect)
        .instrument(info_span!("mutations.await_approval.effect_status"))
        .await
        .unwrap_or(EffectStatus::Emit);
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
    }.instrument(span).await
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
                    Some(req) => {
                        h.handle(req)
                            .instrument(info_span!("mutations.spawn_handler.dispatch"))
                            .await
                    }
                    None      => break,
                }
            }
        }
    }.instrument(info_span!("mutations.spawn_handler.loop")))
}

#[allow(dead_code)]
fn _phase_1_keepalive(_: EffectResult) {}
