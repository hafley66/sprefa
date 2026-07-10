//! Call hierarchy + type hierarchy (Track B, wave 4, item B5):
//!
//!   * `textDocument/prepareCallHierarchy` — cursor -> a callable
//!     `HierarchyItem`, tier "compiler" (a loaded SCIP index, restricted to a
//!     `scip_fn_edge` participant) then tier "resolved" (`call_def`/
//!     `call_name` by identifier text).
//!   * `callHierarchy/incomingCalls` / `callHierarchy/outgoingCalls` — 1-hop
//!     over `call_edge` (resolved tier) or `scip_fn_edge` (compiler tier), no
//!     closure; `fromRanges` from `call_site`.
//!   * `textDocument/prepareTypeHierarchy` — cursor -> a type-shaped
//!     `HierarchyItem` (struct/enum/trait/class/interface only).
//!   * `typeHierarchy/supertypes` / `typeHierarchy/subtypes` — 1-hop over
//!     `type_link` kind `impl` (+ `generic` when the owner is itself an
//!     interface/trait) or `scip_impl` for a compiler-tier item.
//!
//! Drives the built `dl --lsp` over stdio with Content-Length framing, the
//! same harness shape as `lsp_symbols.rs`/`lsp_refs.rs`. Each spawn gets its
//! own temp fixture and db so CI never touches the live checkout or hangs.

use std::fs;
use std::io::{BufRead, BufReader, Read, Write};
use std::path::{Path, PathBuf};
use std::process::{Child, ChildStdout, Command, Stdio};
use std::sync::mpsc;
use std::time::Duration;

const DL: &str = env!("CARGO_BIN_EXE_dl");
const READ_TIMEOUT: Duration = Duration::from_secs(30);

fn sandbox(tag: &str) -> PathBuf {
    let dir = std::env::temp_dir().join(format!("lsp_hierarchy_{tag}"));
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
        let mut cmd = Command::new(DL);
        cmd.arg(prog)
            .args(["--lsp", "--db", db.to_str().unwrap()])
            .current_dir(root)
            .stdin(Stdio::piped())
            .stdout(Stdio::piped())
            .stderr(Stdio::piped());
        let mut child = cmd.spawn().expect("spawn dl --lsp");
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

fn position_params(file: &Path, line: u32, character: u32) -> serde_json::Value {
    serde_json::json!({
        "textDocument": {"uri": format!("file://{}", file.to_str().unwrap())},
        "position": {"line": line, "character": character}
    })
}

/// The regex identifier-locator + type/call family triggers every test
/// program in this file shares: `sym` locates every identifier-shaped token
/// into the ref spine (so `span_at` finds something under the cursor), `te`
/// forces the type family (`type_entity`/`type_link`/...), `cs` forces the
/// call family (`call_def`/`call_edge`/`call_site`/`call_name`).
fn hierarchy_program() -> String {
    concat!(
        "rel sym(name: text, f: file, l: int).\n",
        "sym(name, f, l) <- scan(\"WORK\", \"src/**/*.rs\", f, rev), ",
        "match(f, rev, /(?<name>[A-Za-z_][A-Za-z0-9_]*)/, l).\n",
        "rel te(name: text).\n",
        "te(name) <- type_entity(_, _, name, _, _, _, _).\n",
        "rel cs(callee: text).\n",
        "cs(callee) <- call_site(_, _, callee, _, _).\n",
    ).to_string()
}

/// Fixture (1): a free function `helper` and its one caller `caller`.
///
/// ```text
/// 0: fn helper() {}
/// 1: (blank)
/// 2: fn caller() {
/// 3:     helper();
/// 4: }
/// ```
fn write_call_fixture(root: &Path) -> PathBuf {
    fs::create_dir_all(root.join("src")).unwrap();
    fs::write(root.join("src/lib.rs"), concat!(
        "fn helper() {}\n",
        "\n",
        "fn caller() {\n",
        "    helper();\n",
        "}\n",
    )).unwrap();
    let prog = root.join("p.dl");
    fs::write(&prog, hierarchy_program()).unwrap();
    prog
}

/// Fixture (2): a trait, a unit struct implementing it.
///
/// ```text
/// 0: trait Speaker {}
/// 1: (blank)
/// 2: struct Dog;
/// 3: (blank)
/// 4: impl Speaker for Dog {}
/// ```
fn write_type_fixture(root: &Path) -> PathBuf {
    fs::create_dir_all(root.join("src")).unwrap();
    fs::write(root.join("src/lib.rs"), concat!(
        "trait Speaker {}\n",
        "\n",
        "struct Dog;\n",
        "\n",
        "impl Speaker for Dog {}\n",
    )).unwrap();
    let prog = root.join("p.dl");
    fs::write(&prog, hierarchy_program()).unwrap();
    prog
}

/// Test (1): `textDocument/prepareCallHierarchy` on the `helper` declaration
/// resolves to a single `CallHierarchyItem` named "helper", kind Function (12).
#[test]
fn call_hierarchy_prepare_resolves_function() {
    let root = sandbox("prepare");
    let prog = write_call_fixture(&root);
    let mut s = Session::spawn(&prog, &root, &root.join("h.db"));
    initialize(&mut s, &root);

    // Cursor inside "helper" on its declaration line (line 0, chars 3..9).
    let result = s.request(2, "textDocument/prepareCallHierarchy",
        position_params(&root.join("src/lib.rs"), 0, 5));
    let items = result.as_array().unwrap_or_else(|| panic!(
        "expected a CallHierarchyItem array, got: {result}\nstderr: {}",
        drain_stderr(&mut s.child)));
    assert_eq!(items.len(), 1, "exactly one item resolves: {result}");
    assert_eq!(items[0].get("name").and_then(|n| n.as_str()), Some("helper"));
    assert_eq!(items[0].get("kind").and_then(|k| k.as_u64()), Some(12),
        "helper is a Function symbol (kind 12): {result}");
    assert!(items[0].get("data").is_some(), "data carries the HierarchyItem for the follow-up requests: {result}");

    s.shutdown();
}

/// Test (2): `callHierarchy/incomingCalls` on `helper`'s prepared item finds
/// `caller` as the one incoming call, with a `fromRanges` entry on line 3 (the
/// `helper();` call site inside `caller`'s body).
#[test]
fn call_hierarchy_incoming_finds_caller() {
    let root = sandbox("incoming");
    let prog = write_call_fixture(&root);
    let mut s = Session::spawn(&prog, &root, &root.join("h.db"));
    initialize(&mut s, &root);

    let prepared = s.request(2, "textDocument/prepareCallHierarchy",
        position_params(&root.join("src/lib.rs"), 0, 5));
    let item = prepared.as_array().and_then(|a| a.first()).cloned()
        .unwrap_or_else(|| panic!("prepare on helper failed: {prepared}\nstderr: {}", drain_stderr(&mut s.child)));

    let result = s.request(3, "callHierarchy/incomingCalls", serde_json::json!({"item": item}));
    let calls = result.as_array().unwrap_or_else(|| panic!(
        "expected a CallHierarchyIncomingCall array, got: {result}\nstderr: {}",
        drain_stderr(&mut s.child)));
    assert_eq!(calls.len(), 1, "exactly one caller: {result}");
    let from_name = calls[0].get("from").and_then(|f| f.get("name")).and_then(|n| n.as_str());
    assert_eq!(from_name, Some("caller"), "caller() is the incoming call: {result}");
    let from_lines: Vec<u64> = calls[0].get("fromRanges").and_then(|r| r.as_array())
        .unwrap_or_else(|| panic!("fromRanges missing: {result}"))
        .iter()
        .filter_map(|r| r.get("start").and_then(|p| p.get("line")).and_then(|l| l.as_u64()))
        .collect();
    assert!(from_lines.contains(&3),
        "the call site is on line 3 (`    helper();`): {from_lines:?} / {result}");

    s.shutdown();
}

/// Test (3): `callHierarchy/outgoingCalls` on `caller`'s prepared item finds
/// `helper` as the one outgoing call, `fromRanges` again on line 3 (relative
/// to the CALLER per the LSP spec, regardless of incoming vs outgoing).
#[test]
fn call_hierarchy_outgoing_finds_callee() {
    let root = sandbox("outgoing");
    let prog = write_call_fixture(&root);
    let mut s = Session::spawn(&prog, &root, &root.join("h.db"));
    initialize(&mut s, &root);

    // Cursor inside "caller" on its declaration line (line 2, chars 3..9).
    let prepared = s.request(2, "textDocument/prepareCallHierarchy",
        position_params(&root.join("src/lib.rs"), 2, 5));
    let item = prepared.as_array().and_then(|a| a.first()).cloned()
        .unwrap_or_else(|| panic!("prepare on caller failed: {prepared}\nstderr: {}", drain_stderr(&mut s.child)));

    let result = s.request(3, "callHierarchy/outgoingCalls", serde_json::json!({"item": item}));
    let calls = result.as_array().unwrap_or_else(|| panic!(
        "expected a CallHierarchyOutgoingCall array, got: {result}\nstderr: {}",
        drain_stderr(&mut s.child)));
    assert_eq!(calls.len(), 1, "exactly one callee: {result}");
    let to_name = calls[0].get("to").and_then(|t| t.get("name")).and_then(|n| n.as_str());
    assert_eq!(to_name, Some("helper"), "helper() is the outgoing call: {result}");
    let from_lines: Vec<u64> = calls[0].get("fromRanges").and_then(|r| r.as_array())
        .unwrap_or_else(|| panic!("fromRanges missing: {result}"))
        .iter()
        .filter_map(|r| r.get("start").and_then(|p| p.get("line")).and_then(|l| l.as_u64()))
        .collect();
    assert!(from_lines.contains(&3),
        "the call site is on line 3 inside caller's own body: {from_lines:?} / {result}");

    s.shutdown();
}

/// Test (4): `textDocument/prepareTypeHierarchy` on `Dog` then
/// `typeHierarchy/supertypes` finds `Speaker`, the trait `impl Speaker for Dog`
/// implements (`type_link` kind "impl").
#[test]
fn type_hierarchy_supertypes_finds_implemented_trait() {
    let root = sandbox("supertypes");
    let prog = write_type_fixture(&root);
    let mut s = Session::spawn(&prog, &root, &root.join("h.db"));
    initialize(&mut s, &root);

    // Cursor inside "Dog" on its declaration line (line 2, chars 7..10).
    let prepared = s.request(2, "textDocument/prepareTypeHierarchy",
        position_params(&root.join("src/lib.rs"), 2, 8));
    let item = prepared.as_array().and_then(|a| a.first()).cloned()
        .unwrap_or_else(|| panic!("prepare on Dog failed: {prepared}\nstderr: {}", drain_stderr(&mut s.child)));
    assert_eq!(item.get("name").and_then(|n| n.as_str()), Some("Dog"));

    let result = s.request(3, "typeHierarchy/supertypes", serde_json::json!({"item": item}));
    let supers = result.as_array().unwrap_or_else(|| panic!(
        "expected a TypeHierarchyItem array, got: {result}\nstderr: {}",
        drain_stderr(&mut s.child)));
    assert!(supers.iter().any(|sup| sup.get("name").and_then(|n| n.as_str()) == Some("Speaker")),
        "Speaker is a supertype of Dog: {result}");

    s.shutdown();
}

/// Test (5): `textDocument/prepareCallHierarchy` off a blank line resolves to
/// null (no located identifier under the cursor).
#[test]
fn call_hierarchy_prepare_null_off_whitespace() {
    let root = sandbox("null");
    let prog = write_call_fixture(&root);
    let mut s = Session::spawn(&prog, &root, &root.join("h.db"));
    initialize(&mut s, &root);

    // Line 1 is blank in the call fixture.
    let result = s.request(2, "textDocument/prepareCallHierarchy",
        position_params(&root.join("src/lib.rs"), 1, 0));
    assert!(result.is_null(), "a blank line resolves to null: {result}");

    s.shutdown();
}
