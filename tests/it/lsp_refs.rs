//! References lens (Track B, wave 2): the `dl/refs` request returns a grouped
//! RefLens (tier "resolved" via the type/call graph, tier "textual" via the ref
//! spine), and `textDocument/references` flattens it to `Location[]` while
//! mapping each hit's repo slug to its OWN on-disk root (the multi-repo
//! `root.join` fix).
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
const READ_TIMEOUT: Duration = Duration::from_secs(30);

fn sandbox(tag: &str) -> PathBuf {
    let dir = std::env::temp_dir().join(format!("lsp_refs_{tag}"));
    let _ = fs::remove_dir_all(&dir);
    fs::create_dir_all(&dir).unwrap();
    dir
}

struct Session {
    child: Child,
    rx: mpsc::Receiver<serde_json::Value>,
}

impl Session {
    fn spawn(prog: &Path, root: &Path, db: &Path, config: Option<&Path>) -> Session {
        let mut cmd = Command::new(DL);
        cmd.arg(prog)
            .args(["--lsp", "--db", db.to_str().unwrap()])
            .current_dir(root)
            .stdin(Stdio::piped())
            .stdout(Stdio::piped())
            .stderr(Stdio::piped());
        if let Some(config) = config {
            cmd.env("SPREFA_CONFIG", config);
        }
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

fn refs_params(file: &Path, line: u32, character: u32) -> serde_json::Value {
    serde_json::json!({
        "uri": format!("file://{}", file.to_str().unwrap()),
        "line": line, "character": character
    })
}

fn reference_params(file: &Path, line: u32, character: u32) -> serde_json::Value {
    serde_json::json!({
        "textDocument": {"uri": format!("file://{}", file.to_str().unwrap())},
        "position": {"line": line, "character": character},
        "context": {"includeDeclaration": true}
    })
}

fn roles(hits: &serde_json::Value) -> Vec<String> {
    hits.as_array().unwrap_or(&Vec::new()).iter()
        .filter_map(|h| h.get("role").and_then(|r| r.as_str()).map(String::from))
        .collect()
}

/// Test (1): `dl/refs` over a resolved fixture. Cursor on the `helper`
/// definition returns tier "resolved" with a declaration and a non-text use
/// (the `helper()` call, role "call").
#[test]
fn dl_refs_resolved_tier_has_declaration_and_role() {
    let root = sandbox("resolved");
    fs::create_dir_all(root.join("src")).unwrap();
    fs::write(root.join("src/lib.rs"),
        "fn helper() {}\nfn caller() { helper() }\n").unwrap();
    let prog = root.join("p.dl");
    fs::write(&prog, concat!(
        // Locate the fn-name spans so the cursor sits on a located string.
        "rel sym(name: text, f: file, l: int).\n",
        "sym(name, f, l) <- scan(\"WORK\", \"src/**/*.rs\", f, rev), ",
        "match(f, rev, /fn (?<name>[a-z_]+)/, l).\n",
        // Opt into the type + call graphs so refs_lens resolves by name.
        "rel te(name: text).\n",
        "te(name) <- type_entity(_, _, name, _, _, _, _).\n",
        "rel cs(callee: text).\n",
        "cs(callee) <- call_site(_, _, callee, _, _).\n",
    )).unwrap();

    let mut s = Session::spawn(&prog, &root, &root.join("refs.db"), None);
    initialize(&mut s, &root);

    // Cursor inside `helper` on its def line (line 0, char 5 — bytes 3..9).
    let result = s.request(2, "dl/refs", refs_params(&root.join("src/lib.rs"), 0, 5));
    let lens = result.as_object().unwrap_or_else(|| panic!(
        "expected a RefLens object, got: {result}\nstderr: {}", drain_stderr(&mut s.child)));
    assert_eq!(lens.get("tier").and_then(|t| t.as_str()), Some("resolved"),
        "name resolves through the type/call graph: {result}");
    let declarations = lens.get("declarations").and_then(|d| d.as_array())
        .unwrap_or_else(|| panic!("declarations array: {result}"));
    assert!(!declarations.is_empty(), "helper has a declaration: {result}");
    let use_roles = roles(lens.get("uses").unwrap_or(&serde_json::Value::Null));
    assert!(use_roles.iter().any(|role| role == "call"),
        "the helper() call surfaces as a non-text `call` use: {result}");

    s.shutdown();
}

/// Test (2): textual fallback. An identifier the program never resolves to a
/// type/call symbol (no type_entity opt-in) comes back tier "textual" with the
/// ref-spine same-string spans, role "text".
#[test]
fn dl_refs_textual_fallback_for_unresolved() {
    let root = sandbox("textual");
    fs::create_dir_all(root.join("src")).unwrap();
    fs::write(root.join("src/a.rs"), "fn shared() {}\n").unwrap();
    fs::write(root.join("src/b.rs"), "fn shared() {}\n").unwrap();
    let prog = root.join("p.dl");
    // Only a regex capture — no type/call opt-in, so `shared` resolves to no sym.
    fs::write(&prog, concat!(
        "rel sym(name: text, f: file, l: int).\n",
        "sym(name, f, l) <- scan(\"WORK\", \"src/**/*.rs\", f, rev), ",
        "match(f, rev, /fn (?<name>[a-z_]+)/, l).\n",
    )).unwrap();

    let mut s = Session::spawn(&prog, &root, &root.join("refs.db"), None);
    initialize(&mut s, &root);

    // Cursor inside `shared` in a.rs (line 0, char 5).
    let result = s.request(2, "dl/refs", refs_params(&root.join("src/a.rs"), 0, 5));
    let lens = result.as_object().unwrap_or_else(|| panic!(
        "expected a RefLens object, got: {result}\nstderr: {}", drain_stderr(&mut s.child)));
    assert_eq!(lens.get("tier").and_then(|t| t.as_str()), Some("textual"),
        "unresolvable identifier falls back to the textual tier: {result}");
    let use_roles = roles(lens.get("uses").unwrap_or(&serde_json::Value::Null));
    assert_eq!(use_roles.len(), 2, "shared occurs in both files: {result}");
    assert!(use_roles.iter().all(|role| role == "text"),
        "textual tier hits are role `text`: {result}");

    s.shutdown();
}

/// Test (3): multi-repo `root.join` fix. Two config repos both hold
/// `src/lib.rs` defining `fn shared`. `textDocument/references` must produce a
/// URI under EACH repo's own root, not two under the primary root.
#[test]
fn references_multi_repo_maps_each_root() {
    let base = sandbox("multi");
    let alpha = base.join("alpha");
    let beta = base.join("beta");
    fs::create_dir_all(alpha.join("src")).unwrap();
    fs::create_dir_all(beta.join("src")).unwrap();
    // Identical content so the ref-spine file id matches in both; only the
    // owning repo root differs, which is exactly what the fix must honor.
    fs::write(alpha.join("src/lib.rs"), "fn shared() {}\n").unwrap();
    fs::write(beta.join("src/lib.rs"), "fn shared() {}\n").unwrap();
    // Canonical roots so repo_roots and the client URI agree on macOS.
    let alpha_c = fs::canonicalize(&alpha).unwrap();
    let beta_c = fs::canonicalize(&beta).unwrap();
    let config = base.join("repos.toml");
    fs::write(&config, format!(
        "[[repos]]\nslug = \"alpha\"\nroot = {alpha:?}\n\
         [[repos]]\nslug = \"beta\"\nroot = {beta:?}\n",
        alpha = alpha_c.to_str().unwrap(), beta = beta_c.to_str().unwrap())).unwrap();

    let prog = base.join("p.dl");
    fs::write(&prog, concat!(
        // Fan the scan + capture over EVERY config repo (scan("*")).
        "rel sym(name: text, f: file, l: int).\n",
        "sym(name, f, l) <- scan(\"*\", \"WORK\", \"src/**/*.rs\", f, rev), ",
        "match(f, rev, /fn (?<name>[a-z_]+)/, l).\n",
        // Opt into the type graph so `shared` resolves to a declaration per repo.
        "rel te(name: text).\n",
        "te(name) <- type_entity(_, _, name, _, _, _, _).\n",
    )).unwrap();

    // Spawn from alpha; both repos are served via $SPREFA_CONFIG.
    let mut s = Session::spawn(&prog, &alpha_c, &base.join("refs.db"), Some(&config));
    initialize(&mut s, &alpha_c);

    let result = s.request(2, "textDocument/references",
        reference_params(&alpha_c.join("src/lib.rs"), 0, 5));
    let locs = result.as_array().unwrap_or_else(|| panic!(
        "expected a location array, got: {result}\nstderr: {}", drain_stderr(&mut s.child)));
    let uris: Vec<&str> = locs.iter()
        .filter_map(|l| l.get("uri").and_then(|u| u.as_str())).collect();
    assert!(uris.iter().any(|u| u.contains("/alpha/") && u.ends_with("src/lib.rs")),
        "a hit under alpha's own root: {uris:?}");
    assert!(uris.iter().any(|u| u.contains("/beta/") && u.ends_with("src/lib.rs")),
        "a hit under beta's own root (NOT primary-root joined): {uris:?}");

    s.shutdown();
}
