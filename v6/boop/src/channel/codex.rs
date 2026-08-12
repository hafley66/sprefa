//! The codex lane channel: a `codex app-server` child spoken to over
//! newline JSON-RPC. `turn/steer` puts text into the turn already running.

use std::process::{Command, Stdio};
use std::time::Duration;

use anyhow::{Context, Result};
use serde_json::{json, Value};

use crate::channel::jsonrpc::RpcChild;
use crate::channel::{ChannelSpec, Delivery, LaneChannel, TurnEnd};

const CALL_TIMEOUT: Duration = Duration::from_secs(120);

pub struct CodexChannel {
    rpc: RpcChild,
    thread: String,
    turn: Option<String>,
    effort: Option<String>,
}

impl CodexChannel {
    pub fn open(spec: &ChannelSpec) -> Result<CodexChannel> {
        let child = Command::new("codex")
            .arg("app-server")
            .current_dir(&spec.cwd)
            .stdin(Stdio::piped())
            .stdout(Stdio::piped())
            .stderr(Stdio::inherit())
            .spawn()
            .context("spawn codex app-server")?;
        let mut rpc = RpcChild::attach(child)?;
        rpc.call(
            "initialize",
            json!({"clientInfo": {"name": "boop", "version": env!("CARGO_PKG_VERSION")}}),
            CALL_TIMEOUT,
        )?;
        rpc.notify("initialized", json!({}))?;
        let (name, effort) = split_effort(spec.model.as_deref());
        let mut params = json!({
            "cwd": spec.cwd.display().to_string(),
            "sandbox": "danger-full-access",
            "approvalPolicy": "never",
        });
        if let Some(name) = name {
            params["model"] = Value::String(name.to_owned());
        }
        let thread = match &spec.resume {
            Some(id) => {
                params["threadId"] = Value::String(id.clone());
                let reply = rpc.call("thread/resume", params, CALL_TIMEOUT)?;
                thread_id(&reply).unwrap_or_else(|| id.clone())
            }
            None => {
                let reply = rpc.call("thread/start", params, CALL_TIMEOUT)?;
                thread_id(&reply).context("codex thread/start returned no thread id")?
            }
        };
        Ok(CodexChannel {
            rpc,
            thread,
            turn: None,
            effort: effort.map(str::to_owned),
        })
    }
}

impl LaneChannel for CodexChannel {
    fn conversation_id(&self) -> Option<String> {
        Some(self.thread.clone())
    }

    fn start_turn(&mut self, text: &str) -> Result<()> {
        let mut params = json!({
            "threadId": self.thread,
            "input": [{"type": "text", "text": text}],
        });
        if let Some(effort) = &self.effort {
            params["effort"] = Value::String(effort.clone());
        }
        let reply = self.rpc.call("turn/start", params, CALL_TIMEOUT)?;
        self.turn = reply
            .get("turn")
            .and_then(|turn| turn.get("id"))
            .and_then(Value::as_str)
            .map(str::to_owned);
        Ok(())
    }

    fn steer(&mut self, text: &str) -> Result<Delivery> {
        let Some(turn) = self.turn.clone() else {
            return Ok(Delivery::NextTurn);
        };
        let params = json!({
            "threadId": self.thread,
            "expectedTurnId": turn,
            "input": [{"type": "text", "text": text}],
        });
        match self.rpc.call("turn/steer", params, CALL_TIMEOUT) {
            Ok(_) => Ok(Delivery::MidTurn),
            Err(_) => Ok(Delivery::NextTurn),
        }
    }

    fn poll_turn(&mut self, timeout: Duration) -> Result<Option<TurnEnd>> {
        let deadline = std::time::Instant::now() + timeout;
        loop {
            let left = deadline.saturating_duration_since(std::time::Instant::now());
            if left.is_zero() {
                return Ok(None);
            }
            let Some(note) = self.rpc.next_notification(left) else {
                return Ok(None);
            };
            let method = note.get("method").and_then(Value::as_str).unwrap_or("");
            let params = note.get("params").cloned().unwrap_or(Value::Null);
            match method {
                "turn/started" => {
                    self.turn = params
                        .get("turn")
                        .and_then(|turn| turn.get("id"))
                        .and_then(Value::as_str)
                        .map(str::to_owned);
                }
                "turn/completed" => {
                    self.turn = None;
                    let status = params
                        .get("turn")
                        .and_then(|turn| turn.get("status"))
                        .and_then(Value::as_str)
                        .unwrap_or("unknown");
                    return Ok(Some(match status {
                        "failed" => TurnEnd::failed(status),
                        _ => TurnEnd::ok(status),
                    }));
                }
                _ => {}
            }
        }
    }

    fn close(&mut self) -> Result<()> {
        self.rpc.close()
    }
}

/// Split `gpt-5.6-luna@medium` into a model name and a reasoning effort. Only
/// the three efforts codex accepts count as a suffix.
fn split_effort(model: Option<&str>) -> (Option<&str>, Option<&str>) {
    let Some(model) = model.filter(|value| !value.is_empty()) else {
        return (None, None);
    };
    match model.rsplit_once('@') {
        Some((name, effort)) if matches!(effort, "low" | "medium" | "high") => {
            (Some(name), Some(effort))
        }
        _ => (Some(model), None),
    }
}

fn thread_id(reply: &Value) -> Option<String> {
    reply
        .get("thread")
        .and_then(|thread| thread.get("id"))
        .and_then(Value::as_str)
        .map(str::to_owned)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn effort_suffix_splits_only_on_the_three_codex_efforts() {
        assert_eq!(
            split_effort(Some("gpt-5.6-luna@medium")),
            (Some("gpt-5.6-luna"), Some("medium"))
        );
        assert_eq!(
            split_effort(Some("vendor@custom")),
            (Some("vendor@custom"), None)
        );
        assert_eq!(
            split_effort(Some("gpt-5.6-sol")),
            (Some("gpt-5.6-sol"), None)
        );
        assert_eq!(split_effort(None), (None, None));
        assert_eq!(split_effort(Some("")), (None, None));
    }

    #[test]
    fn thread_id_reads_the_nested_field() {
        let reply = json!({"thread": {"id": "019f-abc"}});
        assert_eq!(thread_id(&reply).as_deref(), Some("019f-abc"));
        assert_eq!(thread_id(&json!({})), None);
    }
}
