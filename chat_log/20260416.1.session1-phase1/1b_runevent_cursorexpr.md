# 1b — RunEvent rewrite + CursorExpr

Rewrite `_0_types::RunEvent` to Session-1 variants. Add `CursorExpr`
type. Derive `Clone` on `Pipeline` / `ForkBranch` so CursorExpr can
hold one. Wire `LspPromptBridge` now that RunEvent has MutationPrompt.

## Prereqs
1a complete (mutations module exists so RunEvent can reference
`dyn MutationEffect` and `Approve`).

## Status
Landed (RunEvent rewrite + CursorExpr confirmed in diff). Spot-check:
```
grep -A6 "pub enum RunEvent" v2/src/_0_types.rs
grep "pub struct CursorExpr" v2/src/_0_types.rs
grep -B1 "pub enum Pipeline" v2/src/_5_op.rs   # expect #[derive(Clone)]
```

## Files

### v2/src/_0_types.rs
Replace the old RunEvent (RunStarted / RuleSkipped / RuleStarted /
CursorIn / CursorOut / DiagBatch / FlushStarted / FlushCompleted /
Backpressure / RunCompleted) with:

```rust
pub enum RunEvent {
    Cursor         { expr_name: Option<Arc<str>>, cursor: Cursor },
    ExprDone       { expr_name: Option<Arc<str>> },
    Diag           { diag: Box<dyn crate::Diagnostic> },
    MutationPrompt {
        effect: Arc<dyn crate::mutations::MutationEffect>,
        ack:    tokio::sync::oneshot::Sender<crate::mutations::Approve>,
    },
    Done,
}
```

Delete `SkipReason` and `RunStatus` (only referenced by the old
RunEvent variants). Keep `RewriteKind` (used by `_4_writer::FileEdit`).

Add:
```rust
#[derive(Clone)]
pub struct CursorExpr {
    pub name:     Option<Arc<str>>,
    pub pipeline: crate::_5_op::Pipeline,
}
```

### v2/src/_5_op.rs
Add `#[derive(Clone)]` to:
- `pub enum Pipeline` (line ~68)
- `pub struct ForkBranch` (line ~121)

`LoweredOp` has a manual `impl Clone`; `ChannelSelector` already derives
Clone; sub-variants compose cleanly.

### v2/src/mutations.rs
Ensure `LspPromptBridge` holds the RunEvent broadcast sender and handler
forwards `RunEvent::MutationPrompt`:

```rust
use tokio::sync::{broadcast, mpsc, oneshot};
use crate::_0_types::RunEvent;

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
    async fn handle(&self, req: MutationRequest) {
        let _ = self.events_tx.send(RunEvent::MutationPrompt {
            effect: req.effect,
            ack:    req.ack,
        });
    }
}
```

## Callsite audit
- `grep -rn "RunEvent::" v2/src/` — only stale doc comment in
  `_13_scan_check.rs:33` mentioning `RunEvent::DiagBatch`; safe to
  delete the reference or leave as historical.
- `grep -rn "EventSink(Arc::new(" v2/src/` — every callback uses `|_|`
  pattern; survives the rewrite.

## Verify
```
cd v2 && cargo build --lib 2>&1 | tail -20
```

## Exit state
- RunEvent has 5 variants: Cursor / ExprDone / Diag / MutationPrompt / Done
- CursorExpr exists
- Pipeline + ForkBranch derive Clone
- SkipReason and RunStatus deleted
- LspPromptBridge forwards prompts via broadcast
