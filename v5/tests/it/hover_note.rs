//! `hover_note(path, line, col, end_line, end_col, md)` — the diag-shaped
//! built-in SINK: a rule heads it to attach markdown to a source span; the
//! LSP `textDocument/hover` handler appends every matching row's `md` to
//! whatever it already synthesizes at that position. Positions are 0-based,
//! `end_line`/`end_col` inclusive, the same convention as `diag`.
//!
//! Tests:
//!   1. A hover_note-only span: hovering inside it returns the note markdown
//!      (with NO entity match, proving notes alone still produce a hover);
//!      hovering on an unrelated line returns null.
//!   2. A hover_note span that also carries a `type_entity` match: the
//!      response contains BOTH the synthesized entity markdown and the note,
//!      joined by a `---` rule.
//!   3. `rel hover_note(...)` is reserved: the engine bails with the
//!      diag-style "head it directly" guard message.
//!
//! Drives the built `dl --lsp` over stdio with Content-Length framing, the
//! same harness shape as `tests/it/lsp_symbols.rs`.

use std::fs;
use std::io::{BufRead, BufReader, Read, Write};
use std::path::{Path, PathBuf};
use std::process::{Child, ChildStdout, Command, Stdio};
use std::sync::mpsc;
use std::time::Duration;

const DL: &str = env!("CARGO_BIN_EXE_dl");
const READ_TIMEOUT: Duration = Duration::from_secs(30);

fn sandbox(tag: &str) -> PathBuf {
    let dir = std::env::temp_dir().join(format!("hover_note_{tag}"));
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
            if let Ok(Some(_)) = self.child.try_wait() {
                return;
            }
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
            if r.read_line(&mut line).unwrap_or(0) == 0 {
                return;
            }
            let trimmed = line.trim_end();
            if trimmed.is_empty() {
                break;
            }
            if let Some(rest) = trimmed.strip_prefix("Content-Length:") {
                len = rest.trim().parse().unwrap_or(0);
            }
        }
        if len == 0 {
            continue;
        }
        let mut buf = vec![0u8; len];
        if r.read_exact(&mut buf).is_err() {
            return;
        }
        if let Ok(v) = serde_json::from_slice::<serde_json::Value>(&buf) {
            if tx.send(v).is_err() {
                return;
            }
        }
    }
}

fn drain_stderr(child: &mut Child) -> String {
    let _ = child.kill();
    let mut s = String::new();
    if let Some(mut e) = child.stderr.take() {
        let _ = e.read_to_string(&mut s);
    }
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

/// (1) A `hover_note`-only span (no `type_entity` opt-in anywhere in the
/// program): hovering INSIDE the span returns the note markdown even though
/// no entity resolved; hovering on the unrelated second line returns null.
#[test]
fn hover_note_alone_shows_at_its_span_and_nowhere_else() {
    let root = sandbox("alone");
    fs::create_dir_all(root.join("src")).unwrap();
    // Whole-match column span for line 1 covers "struct FocusPoint" (cols
    // 0..17, 0-based exclusive end); line 2 is a different, unrelated struct
    // with no note attached.
    fs::write(
        root.join("src/anchors.rs"),
        "struct FocusPoint;\nstruct OtherThing;\n",
    )
    .unwrap();
    let prog = root.join("p.dl");
    fs::write(&prog, concat!(
        "rel sym(name: text, source_file: file, match_line: int, ",
        "start_col: int, finish_col: int).\n",
        "sym(name, source_file, match_line, start_col, finish_col) <- ",
        "scan(\"WORK\", \"src/**/*.rs\", source_file, rev), ",
        "match(source_file, rev, /struct (?<name>[A-Za-z]+)/, match_line, start_col, finish_col).\n",
        "hover_note(path: source_file, line: match_line - 1, end_line: match_line - 1, ",
        "col: start_col, end_col: finish_col, md: \"FocusPoint carries the anchor position.\") <- ",
        "sym(name, source_file, match_line, start_col, finish_col), name = \"FocusPoint\".\n",
    )).unwrap();

    let mut s = Session::spawn(&prog, &root, &root.join("hover.db"));
    initialize(&mut s, &root);

    // Inside the note span: line 0, character 10 (within "struct FocusPoint").
    let inside = s.request(
        2,
        "textDocument/hover",
        position_params(&root.join("src/anchors.rs"), 0, 10),
    );
    let inside_md = inside
        .get("contents")
        .and_then(|c| c.get("value"))
        .and_then(|v| v.as_str())
        .unwrap_or_else(|| {
            panic!(
                "expected a hover with markdown content, got: {inside}\nstderr: {}",
                drain_stderr(&mut s.child)
            )
        });
    assert!(
        inside_md.contains("FocusPoint carries the anchor position."),
        "the note text is in the hover: {inside_md}"
    );

    // Outside the note span: line 1 (the OTHER struct, no note attached, no
    // entity opted in anywhere in this program) — null.
    let outside = s.request(
        3,
        "textDocument/hover",
        position_params(&root.join("src/anchors.rs"), 1, 10),
    );
    assert!(
        outside.is_null(),
        "no note and no entity at the unrelated line: {outside}"
    );

    s.shutdown();
}

/// (2) A `hover_note` span that coincides with a `type_entity` match (a
/// `struct Anchor;` declaration, opted into via reading `type_entity`): the
/// hover response carries BOTH the synthesized entity markdown and the note,
/// joined by a `---` rule.
#[test]
fn hover_note_merges_with_entity_hover() {
    let root = sandbox("merge");
    fs::create_dir_all(root.join("src")).unwrap();
    fs::write(root.join("src/lib.rs"), "struct Anchor;\n").unwrap();
    let prog = root.join("p.dl");
    // The regex matches only the identifier itself (uppercase-leading run):
    // "struct" is all-lowercase so it never matches `[A-Z][a-zA-Z]*`, and the
    // first (and only) match on the line is "Anchor" — the whole-match
    // columns exactly bound the identifier, same span the entity resolves.
    fs::write(
        &prog,
        concat!(
        "rel sym(name: text, source_file: file, match_line: int, ",
        "start_col: int, finish_col: int).\n",
        "sym(name, source_file, match_line, start_col, finish_col) <- ",
        "scan(\"WORK\", \"src/**/*.rs\", source_file, rev), ",
        "match(source_file, rev, /(?<name>[A-Z][a-zA-Z]*)/, match_line, start_col, finish_col).\n",
        "rel entity_name(name: text).\n",
        "entity_name(name) <- type_entity(_, _, name, _, _, _, _).\n",
        "hover_note(path: source_file, line: match_line - 1, end_line: match_line - 1, ",
        "col: start_col, end_col: finish_col, md: \"Anchor pins the flow start.\") <- ",
        "sym(name, source_file, match_line, start_col, finish_col), name = \"Anchor\".\n",
    ),
    )
    .unwrap();

    let mut s = Session::spawn(&prog, &root, &root.join("hover.db"));
    initialize(&mut s, &root);

    let result = s.request(
        2,
        "textDocument/hover",
        position_params(&root.join("src/lib.rs"), 0, 9),
    );
    let md = result
        .get("contents")
        .and_then(|c| c.get("value"))
        .and_then(|v| v.as_str())
        .unwrap_or_else(|| {
            panic!(
                "expected a hover with markdown content, got: {result}\nstderr: {}",
                drain_stderr(&mut s.child)
            )
        });
    assert!(
        md.contains("Anchor"),
        "the entity markdown names Anchor: {md}"
    );
    assert!(
        md.contains("Anchor pins the flow start."),
        "the note text is present: {md}"
    );
    assert!(
        md.contains("---"),
        "entity and note are separated by a rule: {md}"
    );
    let entity_idx = md
        .find("struct")
        .unwrap_or_else(|| panic!("entity markdown missing: {md}"));
    let note_idx = md
        .find("Anchor pins the flow start.")
        .unwrap_or_else(|| panic!("note text missing: {md}"));
    assert!(
        entity_idx < note_idx,
        "entity markdown comes first, then the note: {md}"
    );

    s.shutdown();
}

fn run(dir: &Path, prog: &str) -> (i32, String, String) {
    fs::write(dir.join("p.dl"), prog).unwrap();
    let out = Command::new(DL)
        .arg(dir.join("p.dl"))
        .args(["--db", dir.join("db").to_str().unwrap(), "--no-daemon"])
        .current_dir(dir)
        .output()
        .expect("run dl");
    (
        out.status.code().unwrap_or(-1),
        String::from_utf8_lossy(&out.stdout).into_owned(),
        String::from_utf8_lossy(&out.stderr).into_owned(),
    )
}

/// (3) `rel hover_note(...)` bails: the sink is reserved, head it directly
/// from a rule, like `diag`.
#[test]
fn rel_decl_of_the_sink_bails() {
    let dir = sandbox("reserved");
    let prog =
        "rel hover_note(path: text, line: int, col: int, end_line: int, end_col: int, md: text).\n";
    let (code, _out, err) = run(&dir, prog);
    assert_ne!(code, 0, "reserved-name decl must fail");
    assert!(
        err.contains("reserved-name")
            && err.contains("relation `hover_note`")
            && err.contains("write to the built-in directly"),
        "{err}"
    );
}
