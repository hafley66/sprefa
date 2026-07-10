//! Nearly-free LSP navigation features (Track B, wave 3, item B2): all three
//! are read-only over existing rels and need no extension changes (VS Code
//! drives them natively).
//!
//!   * B2a `textDocument/documentHighlight` — cursor -> same-string spans WITHIN
//!     the cursor's own file (ref-spine `span_at` + `string_spans`, file-scoped).
//!   * B2b `workspace/symbol` — a substring over `type_entity` names UNION the
//!     callables (`call_def` joined `call_name`), each hit located under its OWN
//!     repo root (the multi-repo idiom).
//!   * B2c `textDocument/documentSymbol` — `type_entity` rows for the file nested
//!     into a hierarchical tree via the parent sym (a method under its owner).
//!
//! Drives the built `dl --lsp` over stdio with Content-Length framing, the same
//! harness shape as `lsp_refs.rs`. Each spawn gets its own temp fixture and db so
//! CI never touches the live checkout or hangs.

use std::fs;
use std::io::{BufRead, BufReader, Read, Write};
use std::path::{Path, PathBuf};
use std::process::{Child, ChildStdout, Command, Stdio};
use std::sync::mpsc;
use std::time::Duration;

const DL: &str = env!("CARGO_BIN_EXE_dl");
const READ_TIMEOUT: Duration = Duration::from_secs(30);

fn sandbox(tag: &str) -> PathBuf {
    let dir = std::env::temp_dir().join(format!("lsp_symbols_{tag}"));
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

fn position_params(file: &Path, line: u32, character: u32) -> serde_json::Value {
    serde_json::json!({
        "textDocument": {"uri": format!("file://{}", file.to_str().unwrap())},
        "position": {"line": line, "character": character}
    })
}

fn document_params(file: &Path) -> serde_json::Value {
    serde_json::json!({
        "textDocument": {"uri": format!("file://{}", file.to_str().unwrap())}
    })
}

/// Test (1) B2a: `textDocument/documentHighlight`. `shared` appears twice in
/// a.rs and once in b.rs; the cursor on a.rs's first occurrence highlights the
/// TWO a.rs spans and nothing from b.rs (the request is inherently file-scoped,
/// so a count of exactly 2 proves the other file's span was excluded).
#[test]
fn document_highlight_is_file_scoped() {
    let root = sandbox("highlight");
    fs::create_dir_all(root.join("src")).unwrap();
    fs::write(root.join("src/a.rs"), "fn shared() {}\nfn shared() {}\n").unwrap();
    fs::write(root.join("src/b.rs"), "fn shared() {}\n").unwrap();
    let prog = root.join("p.dl");
    // A regex capture locates the `shared` spans into the ref spine so the cursor
    // sits on a located string; no type/call opt-in is needed for highlight.
    fs::write(&prog, concat!(
        "rel sym(name: text, f: file, l: int).\n",
        "sym(name, f, l) <- scan(\"WORK\", \"src/**/*.rs\", f, rev), ",
        "match(f, rev, /fn (?<name>[a-z_]+)/, l).\n",
    )).unwrap();

    let mut s = Session::spawn(&prog, &root, &root.join("sym.db"), None);
    initialize(&mut s, &root);

    // Cursor inside the first `shared` in a.rs (line 0, char 5 — bytes 3..9).
    let result = s.request(2, "textDocument/documentHighlight",
        position_params(&root.join("src/a.rs"), 0, 5));
    let highlights = result.as_array().unwrap_or_else(|| panic!(
        "expected a DocumentHighlight array, got: {result}\nstderr: {}",
        drain_stderr(&mut s.child)));
    assert_eq!(highlights.len(), 2,
        "`shared` occurs twice in a.rs and once in b.rs; only a.rs's two spans \
         highlight (b.rs excluded): {result}");
    let start_lines: Vec<u64> = highlights.iter()
        .filter_map(|h| h.get("range").and_then(|r| r.get("start"))
            .and_then(|p| p.get("line")).and_then(|l| l.as_u64()))
        .collect();
    assert!(start_lines.contains(&0) && start_lines.contains(&1),
        "the two highlights are on a.rs lines 0 and 1: {start_lines:?}");

    s.shutdown();
}

/// Test (2) B2b: `workspace/symbol` across two config repos. `alpha` declares a
/// struct and a function, `beta` a function; a substring query finds a type and
/// a function, each located under its OWN repo root (not the primary root).
#[test]
fn workspace_symbol_finds_type_and_function_across_repos() {
    let base = sandbox("workspace");
    let alpha = base.join("alpha");
    let beta = base.join("beta");
    fs::create_dir_all(alpha.join("src")).unwrap();
    fs::create_dir_all(beta.join("src")).unwrap();
    fs::write(alpha.join("src/lib.rs"),
        "struct WidgetThing;\nfn widget_maker() {}\n").unwrap();
    fs::write(beta.join("src/lib.rs"), "fn widget_helper() {}\n").unwrap();
    let alpha_c = fs::canonicalize(&alpha).unwrap();
    let beta_c = fs::canonicalize(&beta).unwrap();
    let config = base.join("repos.toml");
    fs::write(&config, format!(
        "[[repos]]\nslug = \"alpha\"\nroot = {alpha:?}\n\
         [[repos]]\nslug = \"beta\"\nroot = {beta:?}\n",
        alpha = alpha_c.to_str().unwrap(), beta = beta_c.to_str().unwrap())).unwrap();

    let prog = base.join("p.dl");
    // Fan the scan over every config repo and opt into BOTH the type graph (for
    // the struct) and the call graph (for the functions).
    fs::write(&prog, concat!(
        "rel sym(name: text, f: file, l: int).\n",
        "sym(name, f, l) <- scan(\"*\", \"WORK\", \"src/**/*.rs\", f, rev), ",
        "match(f, rev, /fn (?<name>[a-z_]+)/, l).\n",
        "rel te(name: text).\n",
        "te(name) <- type_entity(_, _, name, _, _, _, _).\n",
        "rel cs(callee: text).\n",
        "cs(callee) <- call_site(_, _, callee, _, _).\n",
    )).unwrap();

    let mut s = Session::spawn(&prog, &alpha_c, &base.join("sym.db"), Some(&config));
    initialize(&mut s, &alpha_c);

    let result = s.request(2, "workspace/symbol", serde_json::json!({"query": "widget"}));
    let symbols = result.as_array().unwrap_or_else(|| panic!(
        "expected a SymbolInformation array, got: {result}\nstderr: {}",
        drain_stderr(&mut s.child)));

    let uri_of = |name: &str| -> Option<String> {
        symbols.iter()
            .find(|sym| sym.get("name").and_then(|n| n.as_str()) == Some(name))
            .and_then(|sym| sym.get("location").and_then(|loc| loc.get("uri"))
                .and_then(|u| u.as_str()).map(String::from))
    };
    // The struct is found by substring, kind Struct (23), under alpha's own root.
    let widget_kind = symbols.iter()
        .find(|sym| sym.get("name").and_then(|n| n.as_str()) == Some("WidgetThing"))
        .and_then(|sym| sym.get("kind").and_then(|k| k.as_u64()));
    assert_eq!(widget_kind, Some(23), "WidgetThing is a Struct symbol: {result}");
    let alpha_uri = uri_of("WidgetThing")
        .unwrap_or_else(|| panic!("WidgetThing not found: {result}"));
    assert!(alpha_uri.contains("/alpha/") && alpha_uri.ends_with("src/lib.rs"),
        "the type is located under alpha's own root: {alpha_uri}");
    // The beta function is found by substring, located under beta's own root.
    let beta_uri = uri_of("widget_helper")
        .unwrap_or_else(|| panic!("widget_helper not found: {result}"));
    assert!(beta_uri.contains("/beta/") && beta_uri.ends_with("src/lib.rs"),
        "the function is located under beta's own root (NOT primary-root joined): {beta_uri}");

    s.shutdown();
}

/// Test (3) B2c: `textDocument/documentSymbol` returns a NESTED tree — the
/// method sits under its owner type (the parent sym joins `type_entity.sym`).
#[test]
fn document_symbol_nests_method_under_owner() {
    let root = sandbox("docsym");
    fs::create_dir_all(root.join("src")).unwrap();
    fs::write(root.join("src/lib.rs"),
        "struct Owner;\nimpl Owner {\n    fn method_a(&self) {}\n}\n").unwrap();
    let prog = root.join("p.dl");
    // Opt into the type graph so `struct Owner` and `fn method_a` become
    // type_entity rows (the method's parent is Owner's own sym).
    fs::write(&prog, concat!(
        "rel sym(name: text, f: file, l: int).\n",
        "sym(name, f, l) <- scan(\"WORK\", \"src/**/*.rs\", f, rev), ",
        "match(f, rev, /fn (?<name>[a-z_]+)/, l).\n",
        "rel te(name: text).\n",
        "te(name) <- type_entity(_, _, name, _, _, _, _).\n",
    )).unwrap();

    let mut s = Session::spawn(&prog, &root, &root.join("sym.db"), None);
    initialize(&mut s, &root);

    let result = s.request(2, "textDocument/documentSymbol",
        document_params(&root.join("src/lib.rs")));
    let symbols = result.as_array().unwrap_or_else(|| panic!(
        "expected a DocumentSymbol array, got: {result}\nstderr: {}",
        drain_stderr(&mut s.child)));

    let owner = symbols.iter()
        .find(|sym| sym.get("name").and_then(|n| n.as_str()) == Some("Owner"))
        .unwrap_or_else(|| panic!("Owner is a top-level symbol: {result}"));
    // Owner is a Struct (SymbolKind 23) at the top level.
    assert_eq!(owner.get("kind").and_then(|k| k.as_u64()), Some(23),
        "Owner is a Struct symbol: {result}");
    let children = owner.get("children").and_then(|c| c.as_array())
        .unwrap_or_else(|| panic!("Owner has a children array: {result}"));
    assert!(children.iter().any(|child|
        child.get("name").and_then(|n| n.as_str()) == Some("method_a")
        && child.get("kind").and_then(|k| k.as_u64()) == Some(6)),
        "method_a nests under Owner as a Method symbol (kind 6): {result}");
    // method_a must NOT also appear as a top-level symbol.
    assert!(!symbols.iter().any(|sym|
        sym.get("name").and_then(|n| n.as_str()) == Some("method_a")),
        "the method is nested, not duplicated at the top level: {result}");

    s.shutdown();
}
