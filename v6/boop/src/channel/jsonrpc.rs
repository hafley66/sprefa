//! Newline-delimited JSON-RPC 2.0 over a child's stdio. No framing headers:
//! codex `app-server` writes one JSON object per line.

use std::collections::HashMap;
use std::io::{BufRead, BufReader, Write};
use std::process::{Child, ChildStdin};
use std::sync::mpsc::{channel, Receiver, Sender};
use std::sync::{Arc, Condvar, Mutex};
use std::time::Duration;

use anyhow::{Context, Result};
use serde_json::{json, Value};

/// Responses keyed by request id, plus the condvar the waiter parks on.
type Replies = Arc<(Mutex<HashMap<i64, Value>>, Condvar)>;

/// A JSON-RPC peer running as a child process. The reader thread owns stdout
/// and splits replies from notifications; nothing else reads the pipe.
pub struct RpcChild {
    child: Child,
    stdin: Arc<Mutex<ChildStdin>>,
    next_id: i64,
    replies: Replies,
    notifications: Receiver<Value>,
}

impl RpcChild {
    /// Take over `child`'s stdio and start the reader thread. Server-initiated
    /// requests get an immediate reply so the peer never blocks on this side.
    pub fn attach(mut child: Child) -> Result<RpcChild> {
        let stdin = child.stdin.take().context("rpc child has no stdin")?;
        let stdout = child.stdout.take().context("rpc child has no stdout")?;
        let stdin = Arc::new(Mutex::new(stdin));
        let replies: Replies = Arc::new((Mutex::new(HashMap::new()), Condvar::new()));
        let (sender, notifications): (Sender<Value>, Receiver<Value>) = channel();
        let reader_replies = Arc::clone(&replies);
        let reply_writer = Arc::clone(&stdin);
        std::thread::spawn(move || {
            for line in BufReader::new(stdout).lines() {
                let Ok(line) = line else { break };
                let Ok(value) = serde_json::from_str::<Value>(&line) else {
                    continue;
                };
                let has_id = value.get("id").is_some();
                let is_reply = value.get("result").is_some() || value.get("error").is_some();
                if has_id && is_reply {
                    if let Some(id) = value.get("id").and_then(Value::as_i64) {
                        let (lock, condvar) = &*reader_replies;
                        if let Ok(mut map) = lock.lock() {
                            map.insert(id, value);
                            condvar.notify_all();
                        }
                    }
                } else if has_id {
                    let reply = answer_server_request(&value);
                    if let Ok(mut pipe) = reply_writer.lock() {
                        let _ = writeln!(pipe, "{reply}");
                        let _ = pipe.flush();
                    }
                } else if sender.send(value).is_err() {
                    break;
                }
            }
        });
        Ok(RpcChild {
            child,
            stdin,
            next_id: 0,
            replies,
            notifications,
        })
    }

    fn write_frame(&self, frame: &Value) -> Result<()> {
        let mut pipe = self
            .stdin
            .lock()
            .map_err(|_| anyhow::anyhow!("rpc stdin lock poisoned"))?;
        writeln!(pipe, "{frame}")?;
        pipe.flush()?;
        Ok(())
    }

    /// Send a request and block for its reply.
    pub fn call(&mut self, method: &str, params: Value, timeout: Duration) -> Result<Value> {
        self.next_id += 1;
        let id = self.next_id;
        let frame = json!({"jsonrpc": "2.0", "id": id, "method": method, "params": params});
        self.write_frame(&frame)
            .with_context(|| format!("write rpc {method}"))?;
        let (lock, condvar) = &*self.replies;
        let mut map = lock.lock().map_err(|_| anyhow::anyhow!("rpc lock poisoned"))?;
        let deadline = std::time::Instant::now() + timeout;
        while !map.contains_key(&id) {
            let left = deadline.saturating_duration_since(std::time::Instant::now());
            if left.is_zero() {
                anyhow::bail!("rpc {method} timed out after {timeout:?}");
            }
            let (guard, _) = condvar
                .wait_timeout(map, left)
                .map_err(|_| anyhow::anyhow!("rpc lock poisoned"))?;
            map = guard;
        }
        let reply = map.remove(&id).expect("checked above");
        if let Some(error) = reply.get("error") {
            anyhow::bail!("rpc {method} failed: {error}");
        }
        Ok(reply.get("result").cloned().unwrap_or(Value::Null))
    }

    /// Send a notification; nothing is awaited.
    pub fn notify(&mut self, method: &str, params: Value) -> Result<()> {
        let frame = json!({"jsonrpc": "2.0", "method": method, "params": params});
        self.write_frame(&frame)
            .with_context(|| format!("write rpc note {method}"))?;
        Ok(())
    }

    /// The next server notification, or `None` when `timeout` elapses first.
    pub fn next_notification(&self, timeout: Duration) -> Option<Value> {
        self.notifications.recv_timeout(timeout).ok()
    }

    /// Close stdin and reap the child.
    pub fn close(&mut self) -> Result<i32> {
        let _ = self.child.kill();
        let status = self.child.wait().context("wait rpc child")?;
        Ok(status.code().unwrap_or(-1))
    }
}

/// Approve whatever the peer asks. A lane runs unattended; a request left
/// unanswered wedges the turn forever, which is the worse failure.
fn answer_server_request(request: &Value) -> String {
    let id = request.get("id").cloned().unwrap_or(Value::Null);
    let method = request.get("method").and_then(Value::as_str).unwrap_or("");
    let result = if method.to_ascii_lowercase().contains("approval") {
        json!({"decision": "approved"})
    } else {
        json!({})
    };
    json!({"jsonrpc": "2.0", "id": id, "result": result}).to_string()
}

#[cfg(test)]
mod tests {
    use super::*;

    /// A `cat`-shaped peer echoes nothing back, so the call must time out
    /// rather than hang the supervisor forever.
    #[test]
    fn a_silent_peer_times_out_instead_of_hanging() {
        let child = std::process::Command::new("sh")
            .args(["-c", "sleep 30"])
            .stdin(std::process::Stdio::piped())
            .stdout(std::process::Stdio::piped())
            .spawn()
            .unwrap();
        let mut rpc = RpcChild::attach(child).unwrap();
        let error = rpc
            .call("initialize", json!({}), Duration::from_millis(200))
            .unwrap_err();
        assert!(error.to_string().contains("timed out"), "{error}");
        rpc.close().unwrap();
    }

    #[test]
    fn a_reply_matches_its_request_id() {
        let script = r#"while read -r line; do printf '{"jsonrpc":"2.0","id":1,"result":{"ok":true}}\n'; done"#;
        let child = std::process::Command::new("sh")
            .args(["-c", script])
            .stdin(std::process::Stdio::piped())
            .stdout(std::process::Stdio::piped())
            .spawn()
            .unwrap();
        let mut rpc = RpcChild::attach(child).unwrap();
        let result = rpc
            .call("initialize", json!({}), Duration::from_secs(5))
            .unwrap();
        assert_eq!(result["ok"], Value::Bool(true));
        rpc.close().unwrap();
    }

    #[test]
    fn a_notification_reaches_the_queue() {
        let script = r#"printf '{"jsonrpc":"2.0","method":"turn/completed","params":{"n":7}}\n'; sleep 5"#;
        let child = std::process::Command::new("sh")
            .args(["-c", script])
            .stdin(std::process::Stdio::piped())
            .stdout(std::process::Stdio::piped())
            .spawn()
            .unwrap();
        let mut rpc = RpcChild::attach(child).unwrap();
        let note = rpc.next_notification(Duration::from_secs(5)).unwrap();
        assert_eq!(note["method"], Value::String("turn/completed".into()));
        rpc.close().unwrap();
    }

    #[test]
    fn a_server_request_is_answered_with_approval() {
        let reply = answer_server_request(
            &serde_json::json!({"id": 4, "method": "execCommandApproval", "params": {}}),
        );
        assert!(reply.contains(r#""decision":"approved""#), "{reply}");
    }
}
