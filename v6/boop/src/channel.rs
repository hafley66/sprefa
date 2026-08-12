//! One live conversation per lane, driven the same way whatever the harness.
//! `Harness::open_channel` mints one; `crate::supervise` is the only caller.

use std::path::PathBuf;

use anyhow::Result;

pub mod claude;
pub mod codex;
pub mod jsonrpc;
pub mod kimi;
pub mod opencode;
pub mod tui;

/// Where a delivered message landed relative to the turn that was running.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Delivery {
    /// Accepted into the turn already in flight.
    MidTurn,
    /// Held by the supervisor; a resume turn opens when the running one ends.
    NextTurn,
}

impl Delivery {
    pub fn as_str(self) -> &'static str {
        match self {
            Delivery::MidTurn => "midturn",
            Delivery::NextTurn => "nextturn",
        }
    }
}

/// How one turn ended.
#[derive(Clone, Debug)]
pub struct TurnEnd {
    pub ok: bool,
    /// A one-line reason, printed by the supervisor and never parsed.
    pub detail: String,
}

impl TurnEnd {
    pub fn ok(detail: impl Into<String>) -> TurnEnd {
        TurnEnd {
            ok: true,
            detail: detail.into(),
        }
    }

    pub fn failed(detail: impl Into<String>) -> TurnEnd {
        TurnEnd {
            ok: false,
            detail: detail.into(),
        }
    }
}

/// What one lane conversation needs before its first turn.
#[derive(Clone, Debug)]
pub struct ChannelSpec {
    /// The harness's own model spelling; `None` takes the harness default.
    pub model: Option<String>,
    pub cwd: PathBuf,
    /// An existing conversation to continue instead of starting a new one.
    pub resume: Option<String>,
}

/// One live conversation. Every harness answers the same four calls, so the
/// supervisor holds no harness id and no per-harness branch.
pub trait LaneChannel: Send {
    /// The harness's own id for this conversation, once it exists. Written to
    /// the lane's registry route so a later resume can find it.
    fn conversation_id(&self) -> Option<String>;

    /// Send `text` as a new turn. Called once to open the lane with the brief,
    /// then again for every batch of messages that arrived between turns.
    fn start_turn(&mut self, text: &str) -> Result<()>;

    /// Offer `text` to the turn already in flight. `NextTurn` means the harness
    /// took nothing and the supervisor must re-offer it after `join`.
    fn steer(&mut self, text: &str) -> Result<Delivery>;

    /// Wait up to `timeout` for the running turn to end. `None` means it is
    /// still running, which is when the supervisor offers it new text.
    fn poll_turn(&mut self, timeout: std::time::Duration) -> Result<Option<TurnEnd>>;

    /// Release the harness child.
    fn close(&mut self) -> Result<()>;
}

/// Milliseconds since the epoch.
pub fn now_ms() -> u64 {
    std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .map(|elapsed| elapsed.as_millis() as u64)
        .unwrap_or(0)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn delivery_spells_both_tiers() {
        assert_eq!(Delivery::MidTurn.as_str(), "midturn");
        assert_eq!(Delivery::NextTurn.as_str(), "nextturn");
    }

    #[test]
    fn turn_end_carries_its_verdict() {
        assert!(TurnEnd::ok("done").ok);
        assert!(!TurnEnd::failed("boom").ok);
    }
}
