# 2d — mutation handler bodies

Fill in `InteractiveCli` stdin prompt loop and `LspPromptBridge`
RunEvent forwarder. Also replace the Phase-1 stub `spawn_handler` (which
returns `TaskGuard::noop()`) with the real spawn loop.

## Prereqs

2a–2c (no hard dep; can run in parallel with 2c if desired, but the
2e rewire needs both).

## Scope

```
v2/src/mutations.rs              handle bodies + spawn_handler loop   (~60 LOC delta)
v2/Cargo.toml                    + tokio features "io-std", "io-util"
```

## Files

### v2/src/mutations.rs

Three deltas.

**(1) `InteractiveCli` — real stdin prompt.**

```rust
pub struct InteractiveCli {
    stdin: tokio::sync::Mutex<tokio::io::BufReader<tokio::io::Stdin>>,
}

impl InteractiveCli {
    pub fn new() -> Self {
        Self { stdin: tokio::sync::Mutex::new(tokio::io::BufReader::new(tokio::io::stdin())) }
    }
}

#[async_trait]
impl MutationHandler for InteractiveCli {
    async fn handle(&self, req: MutationRequest) {
        tokio::select! {
            biased;
            _ = req.cancel.cancelled() => { return; }
            _ = self.prompt(&req) => {}
        }
    }
}

impl InteractiveCli {
    async fn prompt(&self, req: &MutationRequest) {
        use tokio::io::{AsyncBufReadExt, AsyncWriteExt};
        let mut stdout = tokio::io::stdout();
        let _ = stdout.write_all(format!("\n─── [{}] ─────\n{}\nApply? [y/N]: ",
            req.effect.kind_sigil(), req.effect.preview_markdown()).as_bytes()).await;
        let _ = stdout.flush().await;
        let mut buf = String::new();
        let mut stdin = self.stdin.lock().await;
        let _ = stdin.read_line(&mut buf).await;
        let ans = if buf.trim().eq_ignore_ascii_case("y") { Approve::Yes } else { Approve::No };
        let _ = req.ack.send(ans);
    }
}
```

Tokio stdin is a single global handle so the Mutex serializes prompts
across concurrent ops. Only one prompt at a time reaches the TTY.

**(2) `LspPromptBridge` — forward as `RunEvent`.**

```rust
pub struct LspPromptBridge {
    events_tx: tokio::sync::broadcast::Sender<crate::_0_types::RunEvent>,
}

impl LspPromptBridge {
    pub fn new(tx: tokio::sync::broadcast::Sender<crate::_0_types::RunEvent>) -> Self {
        Self { events_tx: tx }
    }
}

#[async_trait]
impl MutationHandler for LspPromptBridge {
    async fn handle(&self, req: MutationRequest) {
        let _ = self.events_tx.send(crate::_0_types::RunEvent::MutationPrompt {
            effect: req.effect,
            ack:    req.ack,
        });
    }
}
```

Note: `RunEvent::MutationPrompt` already carries `ack: oneshot::Sender<Approve>`.
Since `broadcast::Sender::send` requires `Clone` on the event, and
`oneshot::Sender` is not `Clone`, `RunEvent` needs the variant sized so
the whole enum is `Clone`.

**Workaround (Z3 deviation)**: use `tokio::sync::mpsc::Sender<RunEvent>`
for LSP, not `broadcast`. Rationale: the LSP client is a single
subscriber; no need for broadcast. Update `LspPromptBridge::new` and
`DocSession` wiring accordingly.

**(3) `spawn_handler` — real spawn loop.**

```rust
pub fn spawn_handler<H: MutationHandler + 'static>(
    h:      Arc<H>,
    mut rx: tokio::sync::mpsc::Receiver<MutationRequest>,
    cancel: tokio_util::sync::CancellationToken,
) -> crate::_task_guard::TaskGuard {
    crate::_task_guard::TaskGuard::spawn(async move {
        loop {
            tokio::select! {
                biased;
                _ = cancel.cancelled() => break,
                maybe = rx.recv() => match maybe {
                    Some(req) => h.handle(req).await,
                    None      => break,
                }
            }
        }
    })
}
```

**(4) `await_approval` — already in Phase 1 skeleton, body was
`todo!("Phase 2")`. Body now:**

```rust
pub async fn await_approval(
    ctx: &crate::_5_op::OpCtx,
    effect: Arc<dyn MutationEffect>,
) -> Result<Approve, Cancelled> {
    let status = ctx.store.effect_status(&*effect).await.unwrap_or(crate::store::EffectStatus::Emit);
    if matches!(status, crate::store::EffectStatus::Skip) {
        return Ok(Approve::Yes);
    }
    let (ack_tx, ack_rx) = tokio::sync::oneshot::channel();
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
```

### v2/Cargo.toml

```
tokio = { workspace = true, features = [..., "io-std", "io-util"] }
```

## Z3 deviations

- **LspPromptBridge uses mpsc, not broadcast.** See above.
- **RunEvent already includes `MutationPrompt { effect, ack }`** (landed
  P1). No variant change needed here; just ensure `RunEvent` is `Send`
  (oneshot::Sender is Send, so yes).

## Tests

Extend `v2/tests/mutations.rs` (may be new):

1. `auto_approve_returns_yes` — already covered by P1 if any test
   existed; port forward.
2. `interactive_cli_rejects_on_enter` — pipe `"\n"` into a test
   `BufReader<Cursor<Vec<u8>>>` wrapped `InteractiveCli`; `handle` runs;
   ack receives `Approve::No`. Need a constructor override accepting
   an `impl AsyncBufRead + Send`. Add `InteractiveCli::with_reader` for
   testability.
3. `interactive_cli_accepts_y` — same, pipe `"y\n"` → `Approve::Yes`.
4. `lsp_prompt_bridge_forwards` — mpsc::channel, send a
   MutationPrompt via bridge, receive on the other end, verify
   `effect.kind_sigil()` matches.
5. `spawn_handler_drops_on_cancel` — spawn, send one req, cancel,
   verify task exits within 10ms, verify the req's ack was never fired
   (the handler's select loop drops the req on cancel before `handle`
   sees it only if cancel races the recv — this is the expected
   "task exits cleanly" invariant).

## Verify

```
cd v2 && cargo test -p v2 --test mutations
```

## Exit state

- `mutations.rs` has zero `todo!()` / `unimplemented!()`
- 5 tests cover Approve paths and spawn lifecycle
- Path is clear for 2e to pass real handlers into `DocSession::new`
