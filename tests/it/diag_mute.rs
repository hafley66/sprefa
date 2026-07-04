//! The `diag_mute` seam: engine-owned writable mute set toggled by the LSP
//! `dl.toggleDiagCode` command, filtered at the publish seam only.
//!
//!   (1) engine unit: `toggle_diag_mute` inserts if absent (returns muted=true),
//!       deletes if present (returns muted=false); a double toggle restores the
//!       empty set. `muted_codes` / `diag_code_states` reflect the set.
//!   (2) LSP e2e: a `workspace/executeCommand` round-trip lists codes, mutes
//!       one, and the republished diagnostics no longer carry it.
//!   (3) `--check` asymmetry: a `diag_mute` row seeded via the engine API does
//!       NOT change what `--check` reports — mute is an editor affordance, not a
//!       CI gate. The publish filter is the only place mute applies.

use sprefa_v5::{db, engine::Engine, lex, parse};
use std::fs;
use std::io::{BufRead, BufReader, Read, Write};
use std::path::{Path, PathBuf};
use std::process::{Child, ChildStdout, Command, Stdio};
use std::sync::mpsc;
use std::time::Duration;

const DL: &str = env!("CARGO_BIN_EXE_dl");
const READ_TIMEOUT: Duration = Duration::from_secs(30);

fn sandbox(tag: &str) -> PathBuf {
    let dir = std::env::temp_dir().join(format!("diag_mute_{tag}_{}", std::process::id()));
    let _ = fs::remove_dir_all(&dir);
    fs::create_dir_all(dir.join("src")).unwrap();
    dir
}

// ── (1) engine-level toggle ─────────────────────────────────────────────────

#[test]
fn toggle_flips_a_row_and_double_toggle_restores() {
    let d = sandbox("toggle");
    // A minimal program so declare_builtins runs (diag_mute is always declared).
    let prog = parse::parse(lex::lex("? true().").unwrap()).unwrap();
    let conn = db::open(Some(d.join("db").to_str().unwrap())).unwrap();
    let mut eng = Engine::new(conn, d.clone());
    eng.tick(&prog, true).unwrap();

    assert!(eng.muted_codes().unwrap().is_empty(), "starts empty");

    // First toggle mutes.
    assert!(eng.toggle_diag_mute("dl-directive").unwrap(), "first toggle -> muted");
    let m = eng.muted_codes().unwrap();
    assert!(m.contains("dl-directive"), "set holds the code: {m:?}");
    assert_eq!(m.len(), 1);

    // Second toggle un-mutes (restores the empty set).
    assert!(!eng.toggle_diag_mute("dl-directive").unwrap(), "second toggle -> un-muted");
    assert!(eng.muted_codes().unwrap().is_empty(), "double toggle restores empty");

    // Distinct codes are independent.
    assert!(eng.toggle_diag_mute("a").unwrap());
    assert!(eng.toggle_diag_mute("b").unwrap());
    assert_eq!(eng.muted_codes().unwrap().len(), 2, "two distinct codes muted");
    assert!(!eng.toggle_diag_mute("a").unwrap());
    let m = eng.muted_codes().unwrap();
    assert!(m.contains("b") && !m.contains("a"), "only b left: {m:?}");
}

#[test]
fn diag_code_states_includes_muted_with_no_finding() {
    let d = sandbox("states");
    let prog = parse::parse(lex::lex("? true().").unwrap()).unwrap();
    let conn = db::open(Some(d.join("db").to_str().unwrap())).unwrap();
    let mut eng = Engine::new(conn, d.clone());
    eng.tick(&prog, true).unwrap();

    // No `diag` rows exist, but a muted code still appears so it can be un-muted.
    eng.toggle_diag_mute("orphan-code").unwrap();
    let states = eng.diag_code_states().unwrap();
    assert!(states.iter().any(|(c, m)| c == "orphan-code" && *m),
        "muted-but-no-finding code is listed as muted: {states:?}");
}

// ── (3) --check is unaffected by a mute row ─────────────────────────────────

/// Seed a `diag_mute` row into a db, then run `dl --check` against that same db
/// and assert the muted code STILL appears in the check output. The publish
/// filter lives in `--lsp`'s `publish`, never in `eng.diags` (which `--check`
/// reads). The `diag` head named-arg lowering needs the full load path, so the
/// db is primed by a real `dl --check` run first; the mute is seeded via the
/// engine API through a trivial priming program on the same db (which is enough
/// to declare the built-in `diag_mute` table).
fn check_broken_import(d: &Path, dbp: &Path, prog_path: &str) -> String {
    let out = Command::new(DL)
        .arg(prog_path)
        .args(["--check", "--root", d.to_str().unwrap(), "--db", dbp.to_str().unwrap(), "--no-daemon"])
        .output()
        .expect("run dl --check");
    String::from_utf8_lossy(&out.stderr).into_owned()
}

#[test]
fn check_output_is_unchanged_with_a_mute_row_present() {
    let d = sandbox("check");
    fs::write(d.join("src/main.rs"), "mod missing;\nfn main() {}\n").unwrap();
    let dbp = d.join("check.db");
    // lint-imports emits a `broken-import` diag for the missing child module.
    let prog_path = concat!(env!("CARGO_MANIFEST_DIR"), "/examples/lint-imports.dl");

    // Baseline: --check reports broken-import (and populates the db / declares
    // the built-in relations including diag_mute).
    let baseline = check_broken_import(&d, &dbp, prog_path);
    assert!(baseline.contains("broken-import"), "baseline reports broken-import:\n{baseline}");

    // Seed the mute through the engine API on the SAME db. A trivial program
    // tick declares the built-ins so the diag_mute seam is live; the tick does
    // not touch the mute set.
    {
        let prog = parse::parse(lex::lex("? true().").unwrap()).unwrap();
        let conn = db::open(Some(dbp.to_str().unwrap())).unwrap();
        let mut eng = Engine::new(conn, d.clone());
        eng.tick(&prog, true).unwrap();
        eng.toggle_diag_mute("broken-import").unwrap();
        assert!(eng.muted_codes().unwrap().contains("broken-import"), "mute seeded");
    }

    // --check against the SAME db, now with `broken-import` muted: the finding
    // STILL reports. Mute is a publish-seam affordance, never a CI gate.
    let after = check_broken_import(&d, &dbp, prog_path);
    assert!(after.contains("broken-import"),
        "--check must ignore diag_mute; expected broken-import in output:\n{after}");
}

// ── (2) LSP executeCommand round-trip ───────────────────────────────────────

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

    /// Drain publishDiagnostics for `uri_tail` until `want` holds, or timeout.
    fn collect_publishes_until(&self, want: impl Fn(&[serde_json::Value]) -> bool) -> Vec<serde_json::Value> {
        let mut pubs = Vec::new();
        loop {
            match self.rx.recv_timeout(READ_TIMEOUT) {
                Ok(v) => {
                    if v.get("method").and_then(|m| m.as_str()) == Some("textDocument/publishDiagnostics") {
                        pubs.push(v);
                        if want(&pubs) { return pubs; }
                    }
                }
                Err(_) => return pubs,
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

fn codes_for(pubs: &[serde_json::Value], uri_tail: &str) -> Vec<String> {
    // The latest publish for the file wins (republish replaces).
    let mut latest: Option<&serde_json::Value> = None;
    for v in pubs {
        let uri = v.get("params").and_then(|p| p.get("uri")).and_then(|u| u.as_str()).unwrap_or("");
        if uri.ends_with(uri_tail) { latest = Some(v); }
    }
    let Some(v) = latest else { return Vec::new() };
    v.get("params").and_then(|p| p.get("diagnostics")).and_then(|d| d.as_array())
        .map(|a| a.iter().filter_map(|d| d.get("code").and_then(|c| c.as_str()).map(String::from)).collect())
        .unwrap_or_default()
}

/// A program emits a `note` diag with code `chatty` on the scanned file. The LSP
/// publishes it; `dl.listDiagCodes` reports it (unmuted); `dl.toggleDiagCode`
/// mutes it; the republished diagnostics for that file no longer carry `chatty`.
#[test]
fn execute_command_mutes_a_code_and_republishes_without_it() {
    let root = sandbox("lsp");
    fs::write(root.join("src/a.rs"), "fn a() {}\n").unwrap();
    let prog = root.join("p.dl");
    fs::write(&prog, concat!(
        "rel seen(p: file).\n",
        "seen(p) <- scan(\"WORK\", \"src/**/*.rs\", p, rev).\n",
        "diag(path: p, line: 1, severity: \"info\", code: \"chatty\", msg: \"hi\") <- seen(p).\n",
    )).unwrap();

    let mut s = Session::spawn(&prog, &root, &root.join("lsp.db"));
    s.request(1, "initialize", serde_json::json!({
        "processId": serde_json::Value::Null,
        "rootUri": format!("file://{}", root.to_str().unwrap()),
        "capabilities": {}
    }));
    s.send(serde_json::json!({"jsonrpc":"2.0","method":"initialized","params":{}}));

    // Cold-tick publish carries `chatty` on src/a.rs.
    let pubs = s.collect_publishes_until(|ps| !codes_for(ps, "src/a.rs").is_empty());
    assert!(codes_for(&pubs, "src/a.rs").contains(&"chatty".to_string()),
        "cold publish carries chatty: {}", drain_stderr(&mut s.child));

    // listDiagCodes reports chatty as unmuted.
    let list = s.request(2, "workspace/executeCommand",
        serde_json::json!({"command":"dl.listDiagCodes","arguments":[]}));
    let arr = list.as_array().unwrap_or_else(|| panic!("expected an array: {list}"));
    assert!(arr.iter().any(|e| e.get("code").and_then(|c| c.as_str()) == Some("chatty")
        && e.get("muted").and_then(|m| m.as_bool()) == Some(false)),
        "chatty listed unmuted: {arr:?}");

    // Toggle mutes it (returns muted=true).
    let toggled = s.request(3, "workspace/executeCommand",
        serde_json::json!({"command":"dl.toggleDiagCode","arguments":["chatty"]}));
    assert_eq!(toggled.get("muted").and_then(|m| m.as_bool()), Some(true),
        "toggle returns muted=true: {toggled}");

    // The toggle triggers a republish; the file's diagnostics no longer carry chatty.
    let pubs = s.collect_publishes_until(|ps| {
        // Wait for a publish that reflects the mute (chatty gone). The republish
        // sends an empty diagnostics array for src/a.rs.
        ps.iter().rev().take(3).any(|v| {
            let uri = v.get("params").and_then(|p| p.get("uri")).and_then(|u| u.as_str()).unwrap_or("");
            uri.ends_with("src/a.rs")
        }) && !codes_for(ps, "src/a.rs").contains(&"chatty".to_string())
    });
    assert!(!codes_for(&pubs, "src/a.rs").contains(&"chatty".to_string()),
        "after mute, chatty is filtered from the republish: {:?}", codes_for(&pubs, "src/a.rs"));

    // listDiagCodes now reports chatty as muted.
    let list = s.request(4, "workspace/executeCommand",
        serde_json::json!({"command":"dl.listDiagCodes","arguments":[]}));
    let arr = list.as_array().unwrap();
    assert!(arr.iter().any(|e| e.get("code").and_then(|c| c.as_str()) == Some("chatty")
        && e.get("muted").and_then(|m| m.as_bool()) == Some(true)),
        "chatty now listed muted: {arr:?}");

    s.shutdown();
}
