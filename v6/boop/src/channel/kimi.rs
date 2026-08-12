//! The kimi lane channel: one `kimi -p` child per turn. `kimi -p` reads its
//! prompt from the flag and never reads stdin, so steer text lands next turn.

use std::io::{BufRead, BufReader};
use std::path::PathBuf;
use std::process::{Child, Command, Stdio};
use std::sync::mpsc::{channel, Receiver};

use anyhow::{Context, Result};
use serde_json::Value;

use crate::channel::{ChannelSpec, Delivery, LaneChannel, TurnEnd};

pub struct KimiChannel {
    cwd: PathBuf,
    model: Option<String>,
    session: Option<String>,
    turn: Option<Child>,
    lines: Option<Receiver<Value>>,
}

impl KimiChannel {
    pub fn open(spec: &ChannelSpec) -> Result<KimiChannel> {
        Ok(KimiChannel {
            cwd: spec.cwd.clone(),
            model: spec.model.clone(),
            session: spec.resume.clone(),
            turn: None,
            lines: None,
        })
    }
}

impl LaneChannel for KimiChannel {
    fn conversation_id(&self) -> Option<String> {
        self.session.clone()
    }

    fn start_turn(&mut self, text: &str) -> Result<()> {
        if self.turn.is_some() {
            anyhow::bail!("a kimi turn is already running");
        }
        let mut command = Command::new("kimi");
        command.args(["--output-format", "stream-json"]);
        if let Some(model) = self.model.as_deref().filter(|value| !value.is_empty()) {
            command.args(["-m", model]);
        }
        if let Some(session) = &self.session {
            command.args(["-S", session]);
        }
        command.args(["-p", text]);
        let mut child = command
            .current_dir(&self.cwd)
            .stdin(Stdio::null())
            .stdout(Stdio::piped())
            .stderr(Stdio::inherit())
            .spawn()
            .context("spawn kimi prompt turn")?;
        let stdout = child.stdout.take().context("kimi child has no stdout")?;
        let (sender, receiver) = channel();
        std::thread::spawn(move || {
            for line in BufReader::new(stdout).lines() {
                let Ok(line) = line else { break };
                println!("{line}");
                let Ok(value) = serde_json::from_str::<Value>(&line) else {
                    continue;
                };
                if sender.send(value).is_err() {
                    break;
                }
            }
        });
        self.turn = Some(child);
        self.lines = Some(receiver);
        Ok(())
    }

    fn steer(&mut self, _text: &str) -> Result<Delivery> {
        Ok(Delivery::NextTurn)
    }

    fn poll_turn(&mut self, timeout: std::time::Duration) -> Result<Option<TurnEnd>> {
        if let Some(lines) = self.lines.as_ref() {
            while let Ok(value) = lines.try_recv() {
                if let Some(id) = session_id_of(&value) {
                    self.session = Some(id);
                }
            }
        }
        let Some(turn) = self.turn.as_mut() else {
            return Ok(Some(TurnEnd::failed("no kimi turn to join")));
        };
        let Some(status) =
            crate::channel::opencode::wait_for(turn, timeout).context("wait kimi turn")?
        else {
            return Ok(None);
        };
        self.turn = None;
        if let Some(lines) = self.lines.take() {
            while let Ok(value) = lines.recv() {
                if let Some(id) = session_id_of(&value) {
                    self.session = Some(id);
                }
            }
        }
        Ok(Some(match status {
            0 => TurnEnd::ok("rc=0"),
            other => TurnEnd::failed(format!("rc={other}")),
        }))
    }

    fn close(&mut self) -> Result<()> {
        if let Some(mut turn) = self.turn.take() {
            let _ = turn.kill();
            let _ = turn.wait();
        }
        Ok(())
    }
}

/// kimi announces its session on a `session.resume_hint` meta line.
fn session_id_of(value: &Value) -> Option<String> {
    if value.get("role").and_then(Value::as_str)? != "meta" {
        return None;
    }
    value
        .get("session_id")
        .and_then(Value::as_str)
        .map(str::to_owned)
}

#[cfg(test)]
mod tests {
    use super::*;

    fn spec() -> ChannelSpec {
        ChannelSpec {
            model: None,
            cwd: std::env::temp_dir(),
            resume: None,
        }
    }

    /// The exact line kimi emits, captured from
    /// `kimi -p "Say exactly OK" --output-format stream-json`.
    #[test]
    fn the_resume_hint_line_yields_the_session_id() {
        let line = r#"{"role":"meta","type":"session.resume_hint","session_id":"session_02ff5485-ab77-4912-b218-0235aad30883","command":"kimi -r session_02ff5485-ab77-4912-b218-0235aad30883"}"#;
        let value: Value = serde_json::from_str(line).unwrap();
        assert_eq!(
            session_id_of(&value).as_deref(),
            Some("session_02ff5485-ab77-4912-b218-0235aad30883")
        );
    }

    #[test]
    fn an_assistant_line_carries_no_session_id() {
        let value: Value = serde_json::from_str(r#"{"role":"assistant","content":"OK"}"#).unwrap();
        assert_eq!(session_id_of(&value), None);
    }

    #[test]
    fn steer_reports_the_next_turn_tier() {
        let mut channel = KimiChannel::open(&spec()).unwrap();
        assert_eq!(channel.steer("hello").unwrap(), Delivery::NextTurn);
    }
}
