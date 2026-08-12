//! The claude lane channel: one long-lived `claude -p` child in stream-json
//! mode. Extra user lines written to its stdin land inside the running turn.

use std::io::{BufRead, BufReader, Write};
use std::process::{Child, ChildStdin, Command, Stdio};
use std::sync::mpsc::{channel, Receiver};

use anyhow::{Context, Result};
use serde_json::{json, Value};

use crate::channel::{ChannelSpec, Delivery, LaneChannel, TurnEnd};

pub struct ClaudeChannel {
    child: Child,
    stdin: ChildStdin,
    events: Receiver<Value>,
    conversation: String,
}

impl ClaudeChannel {
    pub fn open(spec: &ChannelSpec) -> Result<ClaudeChannel> {
        let conversation = spec.resume.clone().unwrap_or_else(new_uuid);
        let mut command = Command::new("claude");
        command
            .arg("-p")
            .args(["--input-format", "stream-json"])
            .args(["--output-format", "stream-json"])
            .arg("--verbose")
            .arg("--dangerously-skip-permissions");
        match &spec.resume {
            Some(id) => {
                command.args(["--resume", id]);
            }
            None => {
                command.args(["--session-id", &conversation]);
            }
        }
        if let Some(model) = spec.model.as_deref().filter(|value| !value.is_empty()) {
            command.args(["--model", model]);
        }
        let mut child = command
            .current_dir(&spec.cwd)
            .stdin(Stdio::piped())
            .stdout(Stdio::piped())
            .stderr(Stdio::inherit())
            .spawn()
            .context("spawn claude stream-json child")?;
        let stdin = child.stdin.take().context("claude child has no stdin")?;
        let stdout = child.stdout.take().context("claude child has no stdout")?;
        let (sender, events) = channel();
        std::thread::spawn(move || {
            for line in BufReader::new(stdout).lines() {
                let Ok(line) = line else { break };
                let Ok(value) = serde_json::from_str::<Value>(&line) else {
                    continue;
                };
                if sender.send(value).is_err() {
                    break;
                }
            }
        });
        Ok(ClaudeChannel {
            child,
            stdin,
            events,
            conversation,
        })
    }

    fn write_user(&mut self, text: &str) -> Result<()> {
        let frame = json!({
            "type": "user",
            "message": {"role": "user", "content": [{"type": "text", "text": text}]}
        });
        writeln!(self.stdin, "{frame}").context("write claude user line")?;
        self.stdin.flush().context("flush claude stdin")?;
        Ok(())
    }
}

impl LaneChannel for ClaudeChannel {
    fn conversation_id(&self) -> Option<String> {
        Some(self.conversation.clone())
    }

    fn start_turn(&mut self, text: &str) -> Result<()> {
        self.write_user(text)
    }

    fn steer(&mut self, text: &str) -> Result<Delivery> {
        self.write_user(text)?;
        Ok(Delivery::MidTurn)
    }

    fn poll_turn(&mut self, timeout: std::time::Duration) -> Result<Option<TurnEnd>> {
        let deadline = std::time::Instant::now() + timeout;
        loop {
            let left = deadline.saturating_duration_since(std::time::Instant::now());
            if left.is_zero() {
                return Ok(None);
            }
            let event = match self.events.recv_timeout(left) {
                Ok(event) => event,
                Err(std::sync::mpsc::RecvTimeoutError::Timeout) => return Ok(None),
                Err(std::sync::mpsc::RecvTimeoutError::Disconnected) => {
                    return Ok(Some(TurnEnd::failed(
                        "claude stream closed before a result event",
                    )))
                }
            };
            if let Some(id) = event.get("session_id").and_then(Value::as_str) {
                self.conversation = id.to_owned();
            }
            if event.get("type").and_then(Value::as_str) != Some("result") {
                continue;
            }
            let subtype = event
                .get("subtype")
                .and_then(Value::as_str)
                .unwrap_or("unknown");
            let errored = event
                .get("is_error")
                .and_then(Value::as_bool)
                .unwrap_or(false);
            return Ok(Some(match errored {
                false => TurnEnd::ok(subtype),
                true => TurnEnd::failed(subtype),
            }));
        }
    }

    fn close(&mut self) -> Result<i32> {
        drop(std::mem::replace(&mut self.stdin, blackhole()?));
        let status = self.child.wait().context("wait claude child")?;
        Ok(status.code().unwrap_or(-1))
    }
}

/// A stdin handle to drop into so the real one can be closed without an
/// `Option` field the rest of the impl would have to unwrap.
fn blackhole() -> Result<ChildStdin> {
    let mut child = Command::new("true")
        .stdin(Stdio::piped())
        .spawn()
        .context("spawn placeholder for a closed stdin")?;
    child.stdin.take().context("placeholder has no stdin")
}

/// A version-4 UUID, the only session id shape `--session-id` accepts.
fn new_uuid() -> String {
    let mut bytes = [0u8; 16];
    let nanos = std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .map(|elapsed| elapsed.as_nanos())
        .unwrap_or(0);
    let mixed = (nanos as u64) ^ ((std::process::id() as u64) << 32);
    bytes[..8].copy_from_slice(&mixed.to_be_bytes());
    bytes[8..].copy_from_slice(&(nanos as u64).rotate_left(17).to_be_bytes());
    bytes[6] = (bytes[6] & 0x0f) | 0x40;
    bytes[8] = (bytes[8] & 0x3f) | 0x80;
    let hex: String = bytes.iter().map(|byte| format!("{byte:02x}")).collect();
    format!(
        "{}-{}-{}-{}-{}",
        &hex[0..8],
        &hex[8..12],
        &hex[12..16],
        &hex[16..20],
        &hex[20..32]
    )
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_minted_session_id_is_a_v4_uuid() {
        let id = new_uuid();
        assert_eq!(id.len(), 36, "{id}");
        let parts: Vec<&str> = id.split('-').collect();
        assert_eq!(
            parts.iter().map(|p| p.len()).collect::<Vec<_>>(),
            vec![8, 4, 4, 4, 12],
            "{id}"
        );
        assert!(parts[2].starts_with('4'), "{id}");
        assert!(
            matches!(&parts[3][0..1], "8" | "9" | "a" | "b"),
            "{id} variant nibble"
        );
    }

    #[test]
    fn two_mints_differ() {
        assert_ne!(new_uuid(), new_uuid());
    }
}
