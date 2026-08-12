//! The opencode lane channel: one `opencode run` child per turn. That child
//! binds no control port, so steer text lands on the next turn.

use std::path::{Path, PathBuf};
use std::process::{Child, Command, Stdio};

use anyhow::{Context, Result};

use crate::channel::{ChannelSpec, Delivery, LaneChannel, TurnEnd};

pub struct OpencodeChannel {
    cwd: PathBuf,
    model: Option<String>,
    session: Option<String>,
    turn: Option<Child>,
    /// Epoch millis the current turn started; the session-id lookup only
    /// accepts an opencode session created at or after it.
    turn_started_ms: u64,
}

impl OpencodeChannel {
    pub fn open(spec: &ChannelSpec) -> Result<OpencodeChannel> {
        Ok(OpencodeChannel {
            cwd: spec.cwd.clone(),
            model: spec.model.clone(),
            session: spec.resume.clone(),
            turn: None,
            turn_started_ms: 0,
        })
    }
}

impl LaneChannel for OpencodeChannel {
    fn conversation_id(&self) -> Option<String> {
        self.session.clone()
    }

    fn start_turn(&mut self, text: &str) -> Result<()> {
        if self.turn.is_some() {
            anyhow::bail!("an opencode turn is already running");
        }
        let mut command = Command::new("opencode");
        command.arg("run").arg("--auto");
        if let Some(model) = self.model.as_deref().filter(|value| !value.is_empty()) {
            command.args(["-m", model]);
        }
        if let Some(session) = &self.session {
            command.args(["-s", session]);
        }
        command.arg(text);
        self.turn_started_ms = crate::channel::now_ms();
        self.turn = Some(
            command
                .current_dir(&self.cwd)
                .stdin(Stdio::null())
                .spawn()
                .context("spawn opencode run")?,
        );
        Ok(())
    }

    fn steer(&mut self, _text: &str) -> Result<Delivery> {
        Ok(Delivery::NextTurn)
    }

    fn poll_turn(&mut self, timeout: std::time::Duration) -> Result<Option<TurnEnd>> {
        let Some(turn) = self.turn.as_mut() else {
            return Ok(Some(TurnEnd::failed("no opencode turn to join")));
        };
        let Some(status) = wait_for(turn, timeout).context("wait opencode run")? else {
            return Ok(None);
        };
        self.turn = None;
        if self.session.is_none() {
            self.session = newest_session(&self.cwd, self.turn_started_ms);
        }
        Ok(Some(match status {
            0 => TurnEnd::ok("rc=0"),
            other => TurnEnd::failed(format!("rc={other}")),
        }))
    }

    fn close(&mut self) -> Result<i32> {
        if let Some(mut turn) = self.turn.take() {
            let _ = turn.kill();
            let _ = turn.wait();
        }
        Ok(0)
    }
}

/// Reap `child` if it exits within `timeout`; `None` means still running.
pub(crate) fn wait_for(child: &mut Child, timeout: std::time::Duration) -> Result<Option<i32>> {
    let deadline = std::time::Instant::now() + timeout;
    loop {
        if let Some(status) = child.try_wait()? {
            return Ok(Some(status.code().unwrap_or(-1)));
        }
        if std::time::Instant::now() >= deadline {
            return Ok(None);
        }
        std::thread::sleep(std::time::Duration::from_millis(50));
    }
}

/// The newest opencode session under `cwd` created at or after `since_ms`.
/// opencode owns this store; boop only reads it.
fn newest_session(cwd: &Path, since_ms: u64) -> Option<String> {
    let path = crate::harness::opencode::store_path()?;
    let connection = rusqlite::Connection::open_with_flags(
        &path,
        rusqlite::OpenFlags::SQLITE_OPEN_READ_ONLY | rusqlite::OpenFlags::SQLITE_OPEN_NO_MUTEX,
    )
    .ok()?;
    let directory = cwd.display().to_string();
    connection
        .query_row(
            "SELECT id FROM session
              WHERE directory = ?1 AND time_created >= ?2
              ORDER BY time_created DESC LIMIT 1",
            rusqlite::params![directory, since_ms as i64],
            |row| row.get::<_, String>(0),
        )
        .ok()
}

#[cfg(test)]
mod tests {
    use super::*;

    fn spec() -> ChannelSpec {
        ChannelSpec {
            model: Some("openrouter/deepseek/deepseek-v4-flash-0731".to_owned()),
            cwd: std::env::temp_dir(),
            resume: None,
        }
    }

    #[test]
    fn steer_reports_the_next_turn_tier() {
        let mut channel = OpencodeChannel::open(&spec()).unwrap();
        assert_eq!(channel.steer("hello").unwrap(), Delivery::NextTurn);
    }

    #[test]
    fn polling_without_a_turn_is_a_failed_end_not_a_panic() {
        let mut channel = OpencodeChannel::open(&spec()).unwrap();
        let end = channel
            .poll_turn(std::time::Duration::from_millis(10))
            .unwrap()
            .unwrap();
        assert!(!end.ok);
    }

    #[test]
    fn a_resumed_channel_reports_its_conversation_before_the_first_turn() {
        let mut request = spec();
        request.resume = Some("ses_abc".to_owned());
        let channel = OpencodeChannel::open(&request).unwrap();
        assert_eq!(channel.conversation_id().as_deref(), Some("ses_abc"));
    }
}
