//! `dl/query` paging protocol (A3): the custom LSP request gains optional
//! `limit` / `offset` / `count` params, backward compatible with the bare
//! `{"sql","params"}` form the browser bridge uses.
//!
//! Response shapes:
//!   * `count == true`  -> `{"total": int}`
//!   * `limit` present  -> `{"rows": [[col,...]], "total": int}` (one page + the
//!                          unpaged total)
//!   * neither          -> `{"rows": [[col,...]]}` (exactly the pre-paging form)
//!
//! Drives the built `dl --lsp` over stdio with Content-Length framing, the same
//! harness shape as `lsp_protocol.rs`. Each spawn gets its own temp fixture and
//! db so CI never touches the live checkout or hangs.

use std::fs;
use std::io::{BufRead, BufReader, Read, Write};
use std::path::{Path, PathBuf};
use std::process::{Child, ChildStdout, Command, Stdio};
use std::sync::mpsc;
use std::time::Duration;

const DL: &str = env!("CARGO_BIN_EXE_dl");
const V5: &str = env!("CARGO_MANIFEST_DIR");
const READ_TIMEOUT: Duration = Duration::from_secs(30);

fn sandbox(tag: &str) -> PathBuf {
    let dir = std::env::temp_dir().join(format!("lsp_query_paging_{tag}"));
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

/// A fixture with five scanned files (a..e) so paging has something to window.
/// `seen(p)` sorts by path, so `SELECT p FROM rel_seen ORDER BY p` is stable.
fn spawn_five_file_session(tag: &str) -> (Session, PathBuf) {
    let root = sandbox(tag);
    fs::create_dir_all(root.join("src")).unwrap();
    for name in ["a", "b", "c", "d", "e"] {
        fs::write(root.join(format!("src/{name}.rs")), format!("fn {name}() {{}}\n")).unwrap();
    }
    let prog = root.join("p.dl");
    fs::write(&prog, concat!(
        "rel seen(p: file).\n",
        "seen(p) <- scan(\"WORK\", \"src/**/*.rs\", p, rev).\n",
    )).unwrap();
    let _ = V5; // fixture is self-contained; V5 kept for parity with the harness.
    let mut s = Session::spawn(&prog, &root, &root.join("paging.db"));
    initialize(&mut s, &root);
    (s, root)
}

const ALL_PATHS: [&str; 5] =
    ["src/a.rs", "src/b.rs", "src/c.rs", "src/d.rs", "src/e.rs"];

/// limit + offset returns the right window AND the true unpaged total.
#[test]
fn paged_query_returns_window_and_total() {
    let (mut s, _root) = spawn_five_file_session("page");

    let result = s.request(2, "dl/query", serde_json::json!({
        "sql": "SELECT p FROM rel_seen ORDER BY p", "limit": 2, "offset": 1}));
    let rows = result.get("rows").and_then(|r| r.as_array()).unwrap_or_else(|| panic!(
        "expected rows, got: {result}\nstderr: {}", drain_stderr(&mut s.child)));
    let paths: Vec<&str> = rows.iter()
        .filter_map(|r| r.get(0).and_then(|v| v.as_str())).collect();
    assert_eq!(paths, vec!["src/b.rs", "src/c.rs"], "page = offset 1, limit 2: {rows:?}");
    let total = result.get("total").and_then(|t| t.as_i64());
    assert_eq!(total, Some(5), "total is the unpaged count of all five files: {result}");

    // offset past the end: empty page, total still the full count.
    let result = s.request(3, "dl/query", serde_json::json!({
        "sql": "SELECT p FROM rel_seen ORDER BY p", "limit": 10, "offset": 99}));
    assert_eq!(result.get("rows").and_then(|r| r.as_array()).map(|a| a.len()), Some(0),
        "offset past the end yields no rows: {result}");
    assert_eq!(result.get("total").and_then(|t| t.as_i64()), Some(5),
        "total unchanged by the offset: {result}");

    // limit present, offset omitted -> defaults to 0 (the first page).
    let result = s.request(4, "dl/query", serde_json::json!({
        "sql": "SELECT p FROM rel_seen ORDER BY p", "limit": 2}));
    let paths: Vec<&str> = result.get("rows").and_then(|r| r.as_array()).unwrap()
        .iter().filter_map(|r| r.get(0).and_then(|v| v.as_str())).collect();
    assert_eq!(paths, vec!["src/a.rs", "src/b.rs"], "omitted offset defaults to 0: {result}");

    // Paging composes with the caller's own positional params: the LIMIT/OFFSET
    // placeholders bind AFTER the user param.
    let result = s.request(5, "dl/query", serde_json::json!({
        "sql": "SELECT p FROM rel_seen WHERE p > ? ORDER BY p",
        "params": ["src/a.rs"], "limit": 2, "offset": 0}));
    let paths: Vec<&str> = result.get("rows").and_then(|r| r.as_array()).unwrap()
        .iter().filter_map(|r| r.get(0).and_then(|v| v.as_str())).collect();
    assert_eq!(paths, vec!["src/b.rs", "src/c.rs"], "user param binds before LIMIT/OFFSET: {result}");
    assert_eq!(result.get("total").and_then(|t| t.as_i64()), Some(4),
        "total counts the filtered set (b..e), not all five: {result}");

    s.shutdown();
}

/// count mode returns only the total, no rows key.
#[test]
fn count_query_returns_total_only() {
    let (mut s, _root) = spawn_five_file_session("count");

    let result = s.request(2, "dl/query", serde_json::json!({
        "sql": "SELECT p FROM rel_seen", "count": true}));
    assert_eq!(result.get("total").and_then(|t| t.as_i64()), Some(5),
        "count mode returns the total: {result}");
    assert!(result.get("rows").is_none(), "count mode carries no rows key: {result}");

    // count respects the caller's params.
    let result = s.request(3, "dl/query", serde_json::json!({
        "sql": "SELECT p FROM rel_seen WHERE p = ?", "params": ["src/c.rs"], "count": true}));
    assert_eq!(result.get("total").and_then(|t| t.as_i64()), Some(1),
        "count with a bound param: {result}");

    s.shutdown();
}

/// No `limit`/`count` -> exactly the legacy `{"rows": [...]}` shape, no total.
#[test]
fn legacy_query_shape_unchanged() {
    let (mut s, _root) = spawn_five_file_session("legacy");

    let result = s.request(2, "dl/query", serde_json::json!({
        "sql": "SELECT p FROM rel_seen ORDER BY p"}));
    let rows = result.get("rows").and_then(|r| r.as_array()).unwrap_or_else(|| panic!(
        "expected rows, got: {result}\nstderr: {}", drain_stderr(&mut s.child)));
    let paths: Vec<&str> = rows.iter()
        .filter_map(|r| r.get(0).and_then(|v| v.as_str())).collect();
    assert_eq!(paths, ALL_PATHS.to_vec(), "legacy path returns every row: {rows:?}");
    assert!(result.get("total").is_none(), "legacy path carries no total key: {result}");

    s.shutdown();
}

/// Malformed SQL still surfaces as a JSON-RPC error (null through the helper) in
/// every mode: legacy, paged, and count.
#[test]
fn invalid_sql_errors_in_every_mode() {
    let (mut s, _root) = spawn_five_file_session("bad");

    let legacy = s.request(2, "dl/query",
        serde_json::json!({"sql": "SELECT nope FROM does_not_exist"}));
    assert!(legacy.is_null(), "legacy bad SQL is an error: {legacy}");

    let paged = s.request(3, "dl/query",
        serde_json::json!({"sql": "SELECT nope FROM does_not_exist", "limit": 5}));
    assert!(paged.is_null(), "paged bad SQL is an error: {paged}");

    let counted = s.request(4, "dl/query",
        serde_json::json!({"sql": "SELECT nope FROM does_not_exist", "count": true}));
    assert!(counted.is_null(), "count bad SQL is an error: {counted}");

    s.shutdown();
}
