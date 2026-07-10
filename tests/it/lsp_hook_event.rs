//! `dl/hookEvent` custom LSP request (F2 of the flow-marks-goto-recorder
//! plan): the extension's transport for landing one harness/editor event
//! (the goto-flow recorder, chat marks, ...) in the engine's built-in
//! `hook_event` rel without a `dl --hook` subprocess. Mirrors the daemon's
//! `hook_event` RPC (`src/daemon.rs`), but every field is required (no
//! seq-defaulting) so a malformed request comes back as a JSON-RPC error
//! instead of guessing.
//!
//! Drives the built `dl --lsp` over stdio with Content-Length framing, the
//! same harness shape as `lsp_query_paging.rs`. Each spawn gets its own temp
//! fixture and db so CI never touches the live checkout or hangs.

use std::fs;
use std::io::{BufRead, BufReader, Read, Write};
use std::path::{Path, PathBuf};
use std::process::{Child, ChildStdout, Command, Stdio};
use std::sync::mpsc;
use std::time::Duration;

const DL: &str = env!("CARGO_BIN_EXE_dl");
const READ_TIMEOUT: Duration = Duration::from_secs(30);

fn sandbox(tag: &str) -> PathBuf {
    let dir = std::env::temp_dir().join(format!("lsp_hook_event_{tag}"));
    let _ = fs::remove_dir_all(&dir);
    fs::create_dir_all(&dir).unwrap();
    dir
}

struct Session {
    child: Child,
    rx: mpsc::Receiver<serde_json::Value>,
}

impl Session {
    fn spawn(prog: &Path, root: &Path, db: &Path) -> Session {
        let mut child = Command::new(DL)
            .arg(prog)
            .args(["--lsp", "--db", db.to_str().unwrap()])
            .current_dir(root)
            .stdin(Stdio::piped())
            .stdout(Stdio::piped())
            .stderr(Stdio::piped())
            .spawn()
            .expect("spawn dl --lsp");
        let stdout = child.stdout.take().unwrap();
        let (tx, rx) = mpsc::channel();
        std::thread::spawn(move || read_frames(stdout, tx));
        Session { child, rx }
    }

    fn send(&mut self, msg: serde_json::Value) {
        let body = serde_json::to_vec(&msg).unwrap();
        let stdin = self.child.stdin.as_mut().unwrap();
        write!(stdin, "Content-Length: {}\r\n\r\n", body.len()).unwrap();
        stdin.write_all(&body).unwrap();
        stdin.flush().unwrap();
    }

    fn request(&mut self, id: i64, method: &str, params: serde_json::Value) -> serde_json::Value {
        self.send(serde_json::json!({"jsonrpc":"2.0","id":id,"method":method,"params":params}));
        loop {
            match self.rx.recv_timeout(READ_TIMEOUT) {
                Ok(v) => {
                    if v.get("id").and_then(|i| i.as_i64()) == Some(id) {
                        return v.get("result").cloned().unwrap_or(serde_json::Value::Null);
                    }
                }
                Err(_) => panic!("no response to {method} (id {id}) within {READ_TIMEOUT:?}"),
            }
        }
    }

    fn shutdown(mut self) {
        self.send(serde_json::json!({"jsonrpc":"2.0","id":99,"method":"shutdown","params":{}}));
        self.send(serde_json::json!({"jsonrpc":"2.0","method":"exit","params":{}}));
        for _ in 0..20 {
            if let Ok(Some(_)) = self.child.try_wait() { return; }
            std::thread::sleep(Duration::from_millis(100));
        }
        let _ = self.child.kill();
        let _ = self.child.wait();
    }
}

fn read_frames(stdout: ChildStdout, tx: mpsc::Sender<serde_json::Value>) {
    let mut r = BufReader::new(stdout);
    loop {
        let mut len = 0usize;
        loop {
            let mut line = String::new();
            if r.read_line(&mut line).unwrap_or(0) == 0 { return; }
            let trimmed = line.trim_end();
            if trimmed.is_empty() { break; }
            if let Some(rest) = trimmed.strip_prefix("Content-Length:") {
                len = rest.trim().parse().unwrap_or(0);
            }
        }
        if len == 0 { continue; }
        let mut buf = vec![0u8; len];
        if r.read_exact(&mut buf).is_err() { return; }
        if let Ok(v) = serde_json::from_slice::<serde_json::Value>(&buf) {
            if tx.send(v).is_err() { return; }
        }
    }
}

fn drain_stderr(child: &mut Child) -> String {
    let _ = child.kill();
    let mut s = String::new();
    if let Some(mut e) = child.stderr.take() { let _ = e.read_to_string(&mut s); }
    s
}

fn initialize(s: &mut Session, root: &Path) {
    s.send(serde_json::json!({
        "jsonrpc":"2.0","id":1,"method":"initialize",
        "params":{"processId":serde_json::Value::Null,
                  "rootUri": format!("file://{}", root.to_str().unwrap()),
                  "capabilities":{}}
    }));
    s.send(serde_json::json!({"jsonrpc":"2.0","method":"initialized","params":{}}));
}

/// A program that opts into `hook_event` (undeclared until referenced) and
/// derives one word per "goto" event via term-form `jsonp`, the chat-marks
/// idiom.
fn goto_word_program() -> &'static str {
    concat!(
        "rel goto_word(session: text, seq: int, word: text).\n",
        "goto_word(session, seq, word) <-\n",
        "    hook_event(\"goto\", session, seq, body),\n",
        "    jsonp(body, \"word\", word).\n",
    )
}

/// Test (1): a well-formed `dl/hookEvent` request returns `{"ok": true}`, the
/// row lands in `hook_event`, and the quiet tick that follows derives a rule
/// reading it immediately — no separate save/query round trip needed.
#[test]
fn dl_hook_event_lands_row_and_ticks() {
    let root = sandbox("ok");
    fs::create_dir_all(&root).unwrap();
    let prog = root.join("p.dl");
    fs::write(&prog, goto_word_program()).unwrap();

    let mut s = Session::spawn(&prog, &root, &root.join("hook.db"));
    initialize(&mut s, &root);

    let result = s.request(2, "dl/hookEvent", serde_json::json!({
        "kind": "goto", "session": "take-1", "seq": 1000i64,
        "json": "{\"word\":\"helper\"}",
    }));
    assert_eq!(result.get("ok").and_then(|v| v.as_bool()), Some(true),
        "well-formed hookEvent acks ok: {result}\nstderr: {}", drain_stderr(&mut s.child));

    // The derived rule over hook_event should have run by the time the
    // response came back (same quiet tick as didSave).
    let rows = s.request(3, "dl/query", serde_json::json!({
        "sql": "SELECT session, seq, word FROM rel_goto_word ORDER BY seq"}));
    let rows = rows.get("rows").and_then(|r| r.as_array()).cloned().unwrap_or_else(|| panic!(
        "expected rows, got: {rows}\nstderr: {}", drain_stderr(&mut s.child)));
    assert_eq!(rows.len(), 1, "one goto_word row derived: {rows:?}");
    assert_eq!(rows[0].get(0).and_then(|v| v.as_str()), Some("take-1"));
    assert_eq!(rows[0].get(1).and_then(|v| v.as_i64()), Some(1000));
    assert_eq!(rows[0].get(2).and_then(|v| v.as_str()), Some("helper"));

    // A second event in the same session accumulates rather than replacing.
    let result = s.request(4, "dl/hookEvent", serde_json::json!({
        "kind": "goto", "session": "take-1", "seq": 2000i64,
        "json": "{\"word\":\"caller\"}",
    }));
    assert_eq!(result.get("ok").and_then(|v| v.as_bool()), Some(true));
    let rows = s.request(5, "dl/query", serde_json::json!({
        "sql": "SELECT word FROM rel_goto_word ORDER BY seq"}));
    let words: Vec<&str> = rows.get("rows").and_then(|r| r.as_array()).into_iter().flatten()
        .filter_map(|r| r.get(0).and_then(|v| v.as_str())).collect();
    assert_eq!(words, vec!["helper", "caller"], "events accumulate: {words:?}");

    s.shutdown();
}

/// Test (2): missing/mistyped params come back as a JSON-RPC error (result
/// stays null through the test helper, the same shape `dl/query`'s bad-SQL
/// case uses) rather than a silently-defaulted event.
#[test]
fn dl_hook_event_bad_params_is_error() {
    let root = sandbox("bad");
    fs::create_dir_all(&root).unwrap();
    let prog = root.join("p.dl");
    fs::write(&prog, goto_word_program()).unwrap();

    let mut s = Session::spawn(&prog, &root, &root.join("hook.db"));
    initialize(&mut s, &root);

    // Missing `seq` entirely.
    let result = s.request(2, "dl/hookEvent", serde_json::json!({
        "kind": "goto", "session": "take-1", "json": "{}",
    }));
    assert!(result.is_null(), "missing seq is a JSON-RPC error: {result}");

    // `seq` present but the wrong type (string, not a number).
    let result = s.request(3, "dl/hookEvent", serde_json::json!({
        "kind": "goto", "session": "take-1", "seq": "not-a-number", "json": "{}",
    }));
    assert!(result.is_null(), "mistyped seq is a JSON-RPC error: {result}");

    // Missing `json`.
    let result = s.request(4, "dl/hookEvent", serde_json::json!({
        "kind": "goto", "session": "take-1", "seq": 1000i64,
    }));
    assert!(result.is_null(), "missing json is a JSON-RPC error: {result}");

    // No row should have landed from any of the rejected requests.
    let rows = s.request(5, "dl/query", serde_json::json!({
        "sql": "SELECT word FROM rel_goto_word"}));
    let rows = rows.get("rows").and_then(|r| r.as_array()).cloned().unwrap_or_else(|| panic!(
        "expected rows, got: {rows}\nstderr: {}", drain_stderr(&mut s.child)));
    assert!(rows.is_empty(), "rejected hookEvent requests write nothing: {rows:?}");

    s.shutdown();
}
