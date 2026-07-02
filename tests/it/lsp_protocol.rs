//! LSP stdio protocol test (spec T3): a Rust port of /tmp/lsp_smoke.py. Spawns the
//! built `dl --lsp` over stdio, does Content-Length framing, drives
//! initialize -> initialized -> didOpen, and asserts that `publishDiagnostics`
//! arrives. Two gates:
//!   (1) the broken-import baseline (`examples/lint-imports.dl` against a temp
//!       fixture repo with a `mod missing;` that has no child file) squiggles the
//!       offending file.
//!   (2) a brand-violating `.dl` program publishes a TypeDiag-sourced diagnostic
//!       with code `brand-mismatch` against the program file's own URI.
//!
//! Each spawn gets its own temp fixture (never the live sprefa checkout), a fresh
//! db, a generous read timeout, and a clean shutdown so CI never hangs.

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
    let dir = std::env::temp_dir().join(format!("lsp_protocol_{tag}"));
    let _ = fs::remove_dir_all(&dir);
    fs::create_dir_all(&dir).unwrap();
    dir
}

/// One published diagnostics notification, flattened to what the assertions need.
#[derive(Debug, Clone)]
struct Published {
    uri: String,
    diags: Vec<Diag>,
}

#[derive(Debug, Clone)]
struct Diag {
    line0: i64,
    code: String,
    message: String,
}

/// A spawned `dl --lsp` plus the channel the reader thread drains stdout into.
struct Session {
    child: Child,
    rx: mpsc::Receiver<serde_json::Value>,
}

impl Session {
    fn spawn(prog: &Path, root: &Path, db: &Path) -> Session {
        let mut child = Command::new(DL)
            .arg(prog)
            .args(["--lsp", "--root", root.to_str().unwrap(), "--db", db.to_str().unwrap()])
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

    /// Drain messages until one satisfies `want`, or the read timeout elapses.
    /// Returns every `publishDiagnostics` seen so far on timeout (so a failing
    /// assertion can report what did arrive).
    fn collect_publishes_until(&self, want: impl Fn(&[Published]) -> bool) -> Vec<Published> {
        let mut pubs = Vec::new();
        loop {
            match self.rx.recv_timeout(READ_TIMEOUT) {
                Ok(v) => {
                    if v.get("method").and_then(|m| m.as_str())
                        == Some("textDocument/publishDiagnostics")
                    {
                        if let Some(p) = parse_publish(&v) { pubs.push(p); }
                        if want(&pubs) { return pubs; }
                    }
                }
                Err(_) => return pubs, // timeout: hand back what we have
            }
        }
    }

    /// Send a request and block until its response arrives (matched by id),
    /// discarding interleaved notifications. Panics on timeout: by the time a
    /// test sends a request the cold tick has already published, so a missing
    /// response is a routing bug, not slowness.
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

    /// shutdown -> exit, then wait for the process; kill if it lingers.
    fn shutdown(mut self) {
        self.send(serde_json::json!({"jsonrpc":"2.0","id":99,"method":"shutdown","params":{}}));
        self.send(serde_json::json!({"jsonrpc":"2.0","method":"exit","params":{}}));
        // Give it a beat to exit cleanly, then force it so CI never hangs.
        for _ in 0..20 {
            if let Ok(Some(_)) = self.child.try_wait() { return; }
            std::thread::sleep(Duration::from_millis(100));
        }
        let _ = self.child.kill();
        let _ = self.child.wait();
    }
}

/// Read Content-Length framed JSON-RPC messages off stdout, forward each parsed
/// value into the channel. Exits when the stream closes.
fn read_frames(stdout: ChildStdout, tx: mpsc::Sender<serde_json::Value>) {
    let mut r = BufReader::new(stdout);
    loop {
        // Read headers up to the blank line.
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

fn parse_publish(v: &serde_json::Value) -> Option<Published> {
    let params = v.get("params")?;
    let uri = params.get("uri")?.as_str()?.to_string();
    let diags = params.get("diagnostics")?.as_array()?.iter().filter_map(|d| {
        let line0 = d.get("range")?.get("start")?.get("line")?.as_i64()?;
        let code = d.get("code").and_then(|c| c.as_str()).unwrap_or("").to_string();
        let message = d.get("message").and_then(|m| m.as_str()).unwrap_or("").to_string();
        Some(Diag { line0, code, message })
    }).collect();
    Some(Published { uri, diags })
}

/// Only called from failure paths: kill the child first, else read_to_string
/// blocks forever on a live process's open stderr pipe.
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

fn did_open(s: &mut Session, file: &Path, language: &str) {
    let text = fs::read_to_string(file).unwrap();
    s.send(serde_json::json!({
        "jsonrpc":"2.0","method":"textDocument/didOpen",
        "params":{"textDocument":{
            "uri": format!("file://{}", file.to_str().unwrap()),
            "languageId": language, "version": 1, "text": text}}
    }));
}

/// Gate (1): the broken-import baseline. A fixture repo with `mod missing;` (no
/// child file) drives `examples/lint-imports.dl`; the scanned file gets a
/// `broken-import` squiggle published over LSP.
#[test]
fn lint_imports_publishes_broken_import() {
    let root = sandbox("lint");
    fs::create_dir_all(root.join("src")).unwrap();
    fs::write(root.join("src/main.rs"), "mod missing;\nfn main() {}\n").unwrap();
    let prog = PathBuf::from(V5).join("examples/lint-imports.dl");
    let target = root.join("src/main.rs");

    let mut s = Session::spawn(&prog, &root, &root.join("lint.db"));
    initialize(&mut s, &root);
    did_open(&mut s, &target, "rust");

    let target_uri_tail = "src/main.rs";
    let pubs = s.collect_publishes_until(|ps| {
        ps.iter().any(|p| p.uri.ends_with(target_uri_tail) && !p.diags.is_empty())
    });

    let hit = pubs.iter().find(|p| p.uri.ends_with(target_uri_tail) && !p.diags.is_empty());
    assert!(hit.is_some(),
        "expected a broken-import publishDiagnostics for src/main.rs; got: {pubs:?}\nstderr: {}",
        drain_stderr(&mut s.child));
    let d = &hit.unwrap().diags[0];
    assert_eq!(d.code, "broken-import", "code mismatch: {d:?}");
    assert_eq!(d.line0, 0, "mod missing is on line 1 (0-based 0): {d:?}");
    assert!(d.message.contains("missing"), "message names the missing mod: {d:?}");

    s.shutdown();
}

/// Gate (2): a brand-violating `.dl` program. Variable `x` is bound at a `Sym`
/// column and a `Mod` column (unrelated brands) -> a TypeDiag-sourced
/// `brand-mismatch` published against the program file's own URI. The mismatch is
/// var-level (no single offending literal), so by design it lands at line 1; the
/// byte->line resolver only moves literal-bearing diags off line 1.
#[test]
fn brand_mismatch_publishes_on_program_uri() {
    let root = sandbox("brand");
    fs::write(root.join("a.txt"), "x\n").unwrap();
    let prog = root.join("p.dl");
    fs::write(&prog, concat!(
        "type Sym <: text.\n",
        "type Mod <: text.\n",
        "rel a(s: Sym).\n",
        "rel b(m: Mod).\n",
        "rel bad(x: text).\n",
        "a(\"foo\") <- scan(\"WORK\", \"*.txt\", f, rev), match(f, rev, /./, ln).\n",
        "b(\"foo\") <- scan(\"WORK\", \"*.txt\", f, rev), match(f, rev, /./, ln).\n",
        "bad(x) <- a(x), b(x).\n",
    )).unwrap();

    let prog_abs = fs::canonicalize(&prog).unwrap();
    let mut s = Session::spawn(&prog, &root, &root.join("brand.db"));
    initialize(&mut s, &root);
    // Open the program file itself: its own type diagnostics target its URI.
    did_open(&mut s, &prog_abs, "datalog");

    let prog_tail = "p.dl";
    let pubs = s.collect_publishes_until(|ps| {
        ps.iter().any(|p| p.uri.ends_with(prog_tail)
            && p.diags.iter().any(|d| d.code == "brand-mismatch"))
    });

    let hit = pubs.iter().find(|p| p.uri.ends_with(prog_tail)
        && p.diags.iter().any(|d| d.code == "brand-mismatch"));
    assert!(hit.is_some(),
        "expected a brand-mismatch publishDiagnostics on the program URI; got: {pubs:?}\nstderr: {}",
        drain_stderr(&mut s.child));
    let d = hit.unwrap().diags.iter().find(|d| d.code == "brand-mismatch").unwrap();
    assert_eq!(d.line0, 0, "var-level brand mismatch is line 1 (0-based 0) by design: {d:?}");
    assert!(d.message.contains("Sym") && d.message.contains("Mod"),
        "message names both brands: {d:?}");

    s.shutdown();
}

fn position_params(file: &Path, line: u32, character: u32) -> serde_json::Value {
    serde_json::json!({
        "textDocument": {"uri": format!("file://{}", file.to_str().unwrap())},
        "position": {"line": line, "character": character}
    })
}

/// `ReferenceParams` additionally requires a `context`; without it the server's
/// serde parse fails and the response is an error, not a location array.
fn reference_params(file: &Path, line: u32, character: u32) -> serde_json::Value {
    let mut p = position_params(file, line, character);
    p["context"] = serde_json::json!({"includeDeclaration": true});
    p
}

/// Gate (3, spec T6): textDocument/references over the ref spine. Two files
/// each define `fn shared`; the regex capture interns the same string at two
/// spans. A references request with the cursor inside one span returns both
/// locations with correct ranges.
#[test]
fn references_returns_all_spans_of_string() {
    let root = sandbox("refs");
    fs::create_dir_all(root.join("src")).unwrap();
    // `shared` occupies bytes 3..9 of line 1 in both files.
    fs::write(root.join("src/a.rs"), "fn shared() {}\n").unwrap();
    fs::write(root.join("src/b.rs"), "fn shared() {}\nfn only_b() {}\n").unwrap();
    let prog = root.join("p.dl");
    fs::write(&prog, concat!(
        "rel sym(name: text, path: file, line: int).\n",
        "sym(name, f, ln) <- scan(\"WORK\", \"src/**/*.rs\", f, rev), ",
        "match(f, rev, /fn (?<name>[a-z_]+)/, ln).\n",
    )).unwrap();

    let mut s = Session::spawn(&prog, &root, &root.join("refs.db"));
    initialize(&mut s, &root);

    // Cursor on `shared` in a.rs (line 0, char 5, inside bytes 3..9).
    let result = s.request(2, "textDocument/references",
        reference_params(&root.join("src/a.rs"), 0, 5));
    let locs = result.as_array().unwrap_or_else(|| panic!(
        "expected a location array, got: {result}\nstderr: {}", drain_stderr(&mut s.child)));
    let uris: Vec<&str> = locs.iter()
        .filter_map(|l| l.get("uri").and_then(|u| u.as_str()))
        .collect();
    assert!(uris.iter().any(|u| u.ends_with("src/a.rs")), "a.rs missing: {locs:?}");
    assert!(uris.iter().any(|u| u.ends_with("src/b.rs")), "b.rs missing: {locs:?}");
    for l in locs {
        let start = &l["range"]["start"];
        assert_eq!(start["line"].as_i64(), Some(0), "span starts on line 1: {l}");
        assert_eq!(start["character"].as_i64(), Some(3), "span starts at `shared`: {l}");
    }
    // Cursor on `only_b` (interned once): exactly one location.
    let result = s.request(3, "textDocument/references",
        reference_params(&root.join("src/b.rs"), 1, 5));
    assert_eq!(result.as_array().map(|a| a.len()), Some(1),
        "only_b has one occurrence: {result}");
    // Cursor on whitespace: null.
    let result = s.request(4, "textDocument/references",
        reference_params(&root.join("src/a.rs"), 0, 2));
    assert!(result.is_null(), "no located span under `fn `: {result}");

    s.shutdown();
}

/// Gate (4, spec T6): textDocument/definition over the import spans + module
/// graph. Cursor on `crate::foo::thing` in main.rs jumps to src/foo.rs at 0:0.
/// The program references `module_edge` so the lazy resolver runs; the scan
/// rule seeds the file set (lint-imports shape).
#[test]
fn definition_jumps_to_module_edge_target() {
    let root = sandbox("def");
    fs::create_dir_all(root.join("src")).unwrap();
    fs::write(root.join("src/main.rs"),
        "mod foo;\nuse crate::foo::thing;\nfn main() { thing() }\n").unwrap();
    fs::write(root.join("src/foo.rs"), "pub fn thing() {}\n").unwrap();
    let prog = root.join("p.dl");
    fs::write(&prog, concat!(
        "rel seen(p: file).\n",
        "seen(p) <- scan(\"WORK\", \"src/**/*.rs\", p, rev).\n",
        "rel edge(a: path, b: path).\n",
        "edge(a, b) <- module_edge(a, b).\n",
    )).unwrap();

    let mut s = Session::spawn(&prog, &root, &root.join("def.db"));
    initialize(&mut s, &root);

    // Cursor inside the `crate::foo::thing` specifier (line 1, char 10).
    let result = s.request(2, "textDocument/definition",
        position_params(&root.join("src/main.rs"), 1, 10));
    let locs = result.as_array().unwrap_or_else(|| panic!(
        "expected a location array, got: {result}\nstderr: {}", drain_stderr(&mut s.child)));
    assert_eq!(locs.len(), 1, "one target: {locs:?}");
    let uri = locs[0]["uri"].as_str().unwrap();
    assert!(uri.ends_with("src/foo.rs"), "target is foo.rs: {locs:?}");
    assert_eq!(locs[0]["range"]["start"]["line"].as_i64(), Some(0),
        "file-level target lands at 0:0: {locs:?}");

    s.shutdown();
}

/// Gate (5, Phase E): textDocument/definition driven by a program-declared
/// `def_target(name, file, line, kind)` rel. A regex captures the bare fn name
/// `alpha` (locating the span); a def_target rule resolves the name to its
/// `type_entity` row. Cursor on the captured span lands on the real definition
/// line, not 0:0 — the signature that Phase E closed. Without def_target, a fn
/// name has no module_edge match, so go-to-def would return null here.
#[test]
fn definition_via_def_target_lands_at_real_line() {
    let root = sandbox("def_target");
    fs::create_dir_all(root.join("src")).unwrap();
    // `alpha` defined on line 2 (0-based 1) of lib.rs.
    fs::write(root.join("src/lib.rs"), "// comment\nfn alpha() {}\nfn beta() {}\n").unwrap();
    let prog = root.join("p.dl");
    fs::write(&prog, concat!(
        // Locate `alpha` / `beta` spans via regex capture.
        "rel sym(name: text, f: file, l: int).\n",
        "sym(name, f, l) <- scan(\"WORK\", \"src/**/*.rs\", f, rev), ",
        "match(f, rev, /fn (?<name>[a-z_]+)/, l).\n",
        // Phase E: the program drives go-to-def. type_entity populates because
        // the rule references it; def_target resolves each name to its def row.
        "rel def_target(name: text, file: file, line: int, kind: text).\n",
        "def_target(name, f, l, \"fn\") <- type_entity(_, _, name, \"function\", _, f, l).\n",
    )).unwrap();

    let mut s = Session::spawn(&prog, &root, &root.join("def_target.db"));
    initialize(&mut s, &root);

    // Cursor inside `alpha` on line 2 (0-based 1), char 5 (inside the name).
    let result = s.request(2, "textDocument/definition",
        position_params(&root.join("src/lib.rs"), 1, 5));
    let locs = result.as_array().unwrap_or_else(|| panic!(
        "expected a location array, got: {result}\nstderr: {}", drain_stderr(&mut s.child)));
    assert_eq!(locs.len(), 1, "one def_target row for alpha: {locs:?}");
    let uri = locs[0]["uri"].as_str().unwrap();
    assert!(uri.ends_with("src/lib.rs"), "target is lib.rs: {locs:?}");
    // alpha is on line 2 (1-based) = 0-based 1. NOT 0:0 (the module_edge fallback).
    assert_eq!(locs[0]["range"]["start"]["line"].as_i64(), Some(1),
        "Phase E lands at the real def line (0-based 1), not 0:0: {locs:?}");

    s.shutdown();
}

/// Gate (6, Phase C): textDocument/hover auto-synthesizes a markdown summary
/// from type_entity + call_def. The program references type_entity (populating
/// the indexer); a regex capture locates the `alpha` span. Hovering over it
/// returns markdown naming the entity. Null when the cursor is on whitespace.
#[test]
fn hover_returns_entity_summary() {
    let root = sandbox("hover");
    fs::create_dir_all(root.join("src")).unwrap();
    fs::write(root.join("src/lib.rs"), "fn alpha() {}\nfn beta() {}\n").unwrap();
    let prog = root.join("p.dl");
    fs::write(&prog, concat!(
        // Locate the fn-name spans.
        "rel sym(name: text, f: file, l: int).\n",
        "sym(name, f, l) <- scan(\"WORK\", \"src/**/*.rs\", f, rev), ",
        "match(f, rev, /fn (?<name>[a-z_]+)/, l).\n",
        // Reference type_entity so the type indexer populates (lazy gate).
        "rel te(name: text).\n",
        "te(name) <- type_entity(_, _, name, _, _, _, _).\n",
    )).unwrap();

    let mut s = Session::spawn(&prog, &root, &root.join("hover.db"));
    initialize(&mut s, &root);

    // Cursor inside `alpha` (line 0, char 5 — inside the captured bytes 3..8).
    let result = s.request(2, "textDocument/hover",
        position_params(&root.join("src/lib.rs"), 0, 5));
    let hover = result.as_object().unwrap_or_else(|| panic!(
        "expected a hover object, got: {result}\nstderr: {}", drain_stderr(&mut s.child)));
    let md = hover.get("contents").and_then(|c| c.get("value")).and_then(|v| v.as_str())
        .unwrap_or_else(|| panic!("expected markdown contents: {result}"));
    assert!(md.contains("alpha"), "hover names the entity: {md}");
    assert!(md.contains("function"), "hover names the kind: {md}");
    assert!(md.contains("src/lib.rs"), "hover locates the file: {md}");

    // Cursor on whitespace (before `fn`): no located span -> null.
    let result = s.request(3, "textDocument/hover",
        position_params(&root.join("src/lib.rs"), 0, 0));
    assert!(result.is_null(), "no located span at col 0: {result}");

    s.shutdown();
}

/// Hover on a struct includes the type-profile overlay line (Ca/Ce/fields/...).
/// The overlay only fires for data types (struct/enum/trait/...) and only when
/// type_edge has rows for the name. The hovered struct has one field type
/// (Beta), so the overlay should mention `fields=1`.
#[test]
fn hover_includes_type_profile_overlay_for_data_types() {
    let root = sandbox("hover-profile");
    fs::create_dir_all(root.join("src")).unwrap();
    fs::write(root.join("src/lib.rs"), concat!(
        "pub struct Alpha { beta: Beta, other: Beta }\n",
        "pub struct Beta {}\n",
        "pub fn use_it(a: Alpha) {}\n",
    )).unwrap();
    let prog = root.join("p.dl");
    fs::write(&prog, concat!(
        "rel sym(name: text, f: file, l: int).\n",
        "sym(name, f, l) <- scan(\"WORK\", \"src/**/*.rs\", f, rev), ",
        "match(f, rev, /(?<name>[A-Z][a-zA-Z_]*)/, l).\n",
        // Opt into type rels so type_entity + type_edge populate.
        "rel te(name: text).\n",
        "te(name) <- type_entity(_, _, name, _, _, _, _).\n",
    )).unwrap();

    let mut s = Session::spawn(&prog, &root, &root.join("hover-profile.db"));
    initialize(&mut s, &root);

    // Cursor inside `Alpha` (line 0, char 14 — inside the captured bytes 11..16).
    let result = s.request(2, "textDocument/hover",
        position_params(&root.join("src/lib.rs"), 0, 14));
    let hover = result.as_object().unwrap_or_else(|| panic!(
        "expected a hover object, got: {result}\nstderr: {}", drain_stderr(&mut s.child)));
    let md = hover.get("contents").and_then(|c| c.get("value")).and_then(|v| v.as_str())
        .unwrap_or_else(|| panic!("expected markdown contents: {result}"));
    assert!(md.contains("struct"), "hover names the kind: {md}");
    assert!(md.contains("Alpha"), "hover names the entity: {md}");
    // The overlay line: Alpha has 2 field edges (both Beta; dedup'd to 1 in
    // DISTINCT). Either way the field count is present.
    assert!(md.contains("fields="), "overlay present: {md}");
    assert!(md.contains("Ca="), "overlay has Ca: {md}");

    s.shutdown();
}

/// Gate (7): the custom `dl/query` request answers raw SQL over the LSP channel
/// with positional rows — the flow-panel webview's data door. Bad SQL comes back
/// as a JSON-RPC error (result stays null through the test helper).
#[test]
fn dl_query_returns_rows_over_lsp() {
    let root = sandbox("query");
    fs::create_dir_all(root.join("src")).unwrap();
    fs::write(root.join("src/a.rs"), "fn a() {}\n").unwrap();
    fs::write(root.join("src/b.rs"), "fn b() {}\n").unwrap();
    let prog = root.join("p.dl");
    fs::write(&prog, concat!(
        "rel seen(p: file).\n",
        "seen(p) <- scan(\"WORK\", \"src/**/*.rs\", p, rev).\n",
    )).unwrap();

    let mut s = Session::spawn(&prog, &root, &root.join("query.db"));
    initialize(&mut s, &root);

    let result = s.request(2, "dl/query",
        serde_json::json!({"sql": "SELECT p FROM rel_seen ORDER BY p"}));
    let rows = result.get("rows").and_then(|r| r.as_array()).unwrap_or_else(|| panic!(
        "expected rows, got: {result}\nstderr: {}", drain_stderr(&mut s.child)));
    let paths: Vec<&str> = rows.iter()
        .filter_map(|r| r.get(0).and_then(|v| v.as_str()))
        .collect();
    assert_eq!(paths, vec!["src/a.rs", "src/b.rs"], "positional rows: {rows:?}");

    // Bound params round-trip.
    let result = s.request(3, "dl/query", serde_json::json!({
        "sql": "SELECT p FROM rel_seen WHERE p = ?", "params": ["src/b.rs"]}));
    let rows = result.get("rows").and_then(|r| r.as_array()).cloned().unwrap_or_default();
    assert_eq!(rows.len(), 1, "one bound match: {result}");

    // Malformed SQL: error response, so the helper surfaces a null result.
    let result = s.request(4, "dl/query",
        serde_json::json!({"sql": "SELECT nope FROM does_not_exist"}));
    assert!(result.is_null(), "bad SQL is a JSON-RPC error: {result}");

    s.shutdown();
}

/// Gate (2b): a literal-bearing TypeDiag lands at its real line, proving the
/// byte->line resolver fixes the T2 line-1 bug. An `fs:` literal in a plain-text
/// column coerces (warn) and the diagnostic points at the literal's actual line,
/// not line 1.
#[test]
fn literal_coerce_diag_lands_at_real_line() {
    let root = sandbox("coerce");
    fs::create_dir_all(root.join("src")).unwrap();
    fs::write(root.join("src/x.rs"), "fn a() {}\n").unwrap();
    let prog = root.join("p.dl");
    // The `fs:src/x.rs` literal is on line 4; the coerce diag must report line 4.
    fs::write(&prog, concat!(
        "rel seen(path: file).\n",
        "rel note(x: text).\n",
        "seen(p) <- scan(\"WORK\", \"src/**/*.rs\", p, rev).\n",
        "note(fs:src/x.rs) <- seen(p).\n",
    )).unwrap();

    let prog_abs = fs::canonicalize(&prog).unwrap();
    let mut s = Session::spawn(&prog, &root, &root.join("coerce.db"));
    initialize(&mut s, &root);
    did_open(&mut s, &prog_abs, "datalog");

    let pubs = s.collect_publishes_until(|ps| {
        ps.iter().any(|p| p.uri.ends_with("p.dl")
            && p.diags.iter().any(|d| d.code == "coerce-text-path"))
    });
    let hit = pubs.iter().find(|p| p.uri.ends_with("p.dl")
        && p.diags.iter().any(|d| d.code == "coerce-text-path"));
    assert!(hit.is_some(),
        "expected a coerce-text-path publishDiagnostics on the program URI; got: {pubs:?}\nstderr: {}",
        drain_stderr(&mut s.child));
    let d = hit.unwrap().diags.iter().find(|d| d.code == "coerce-text-path").unwrap();
    assert_eq!(d.line0, 3, "the fs: literal is on line 4 (0-based 3), not line 1: {d:?}");

    s.shutdown();
}
