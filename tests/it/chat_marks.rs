//! `hook_event` seam + `examples/chat-marks.dl`. `dl --hook` feeds each harness
//! event into the built-in `hook_event(kind, session, seq, json)` rel (rows
//! accumulate in the db); the chat-marks program extracts the prompt via
//! term-form `jsonp`, marks messages that open with a plain phrase, and assigns
//! every later message to the nearest preceding mark (argmax). All hermetic:
//! `--no-daemon`, a per-test `--db`, in-process only.

use std::fs;
use std::io::Write;
use std::path::{Path, PathBuf};
use std::process::{Command, Stdio};

const DL: &str = env!("CARGO_BIN_EXE_dl");
const PROG: &str = concat!(env!("CARGO_MANIFEST_DIR"), "/examples/chat-marks.dl");

/// A throwaway git repo + its own db path. Returns (root, db).
fn fixture(tag: &str) -> (PathBuf, PathBuf) {
    let root = fs::canonicalize({
        let d = std::env::temp_dir().join(format!("chatmarks_{tag}"));
        let _ = fs::remove_dir_all(&d);
        fs::create_dir_all(&d).unwrap();
        d
    }).unwrap();
    assert!(Command::new("git").arg("-C").arg(&root).args(["init", "-q"])
        .status().unwrap().success());
    let db = root.join("db");
    (root, db)
}

/// Feed one harness event through `dl --hook` (in-process). Rows accumulate in
/// `db` across calls, so a sequence of `feed`s builds the session.
fn feed(root: &Path, db: &Path, event: &str) {
    let mut child = Command::new(DL)
        .arg("--hook").arg(PROG)
        .args(["--db", db.to_str().unwrap(), "--no-daemon"])
        .current_dir(root)
        .stdin(Stdio::piped()).stdout(Stdio::piped()).stderr(Stdio::piped())
        .spawn().unwrap();
    child.stdin.take().unwrap().write_all(event.as_bytes()).unwrap();
    assert!(child.wait_with_output().unwrap().status.success());
}

/// Run the chat-marks program one-shot over the accumulated db and return
/// stdout. It re-derives from the persisted `hook_event` rows and prints its own
/// `? section_of(...)` query (`?` runs one-shot, stripped only from a hook tick).
fn sections(root: &Path, db: &Path) -> String {
    let out = Command::new(DL)
        .arg(PROG)
        .args(["--db", db.to_str().unwrap(), "--no-daemon"])
        .current_dir(root)
        .output().unwrap();
    String::from_utf8_lossy(&out.stdout).into_owned()
}

/// Query the built-in `hook_event` rel standalone (a built-in needs no program
/// to be declared) and return stdout.
fn hook_events(root: &Path, db: &Path) -> String {
    let qf = root.join("q.dl");
    fs::write(&qf, "? hook_event(kind, session, seq, json).").unwrap();
    let out = Command::new(DL)
        .arg(&qf)
        .args(["--db", db.to_str().unwrap(), "--no-daemon"])
        .current_dir(root)
        .output().unwrap();
    String::from_utf8_lossy(&out.stdout).into_owned()
}

fn prompt_event(session: &str, prompt: &str) -> String {
    format!(
        r#"{{"hook_event_name":"UserPromptSubmit","session_id":"{session}","prompt":"{prompt}"}}"#)
}

#[test]
fn hook_event_accumulates_and_section_assigns_to_mark() {
    let (root, db) = fixture("assign");
    feed(&root, &db, &prompt_event("s1", "@@mark Design pass"));
    feed(&root, &db, &prompt_event("s1", "lets discuss the layout"));

    // Two events landed in the accumulating built-in rel.
    let he = hook_events(&root, &db);
    assert!(he.contains("(2 rows)"), "hook_event should hold 2 rows:\n{he}");

    // The plain message is assigned to the mark's title.
    let so = sections(&root, &db);
    assert!(so.contains("Design pass\tlets discuss the layout"),
        "second message should sit under the mark's title:\n{so}");
}

#[test]
fn second_mark_flips_the_argmax() {
    let (root, db) = fixture("flip");
    feed(&root, &db, &prompt_event("s1", "@@mark First"));
    feed(&root, &db, &prompt_event("s1", "under first"));
    feed(&root, &db, &prompt_event("s1", "@@mark Second"));
    feed(&root, &db, &prompt_event("s1", "under second"));

    let so = sections(&root, &db);
    // Earlier message stays with the earlier mark; later message follows the flip.
    assert!(so.contains("First\tunder first"), "message 2 under First:\n{so}");
    assert!(so.contains("Second\tunder second"), "message 4 under Second (flip):\n{so}");
    assert!(!so.contains("First\tunder second"), "the flip must reassign message 4:\n{so}");
}

#[test]
fn non_prompt_event_lands_but_is_ignored_by_sections() {
    let (root, db) = fixture("generality");
    feed(&root, &db, &prompt_event("s1", "@@mark Only mark"));
    feed(&root, &db, &prompt_event("s1", "a real message"));
    // A different event kind: it must still land in the generic hook_event rel.
    feed(&root, &db,
        r#"{"hook_event_name":"PostToolUse","session_id":"s1","tool_name":"Read"}"#);

    let he = hook_events(&root, &db);
    assert!(he.contains("(3 rows)"), "every event kind lands in hook_event:\n{he}");
    assert!(he.contains("PostToolUse"), "PostToolUse row present:\n{he}");

    // section_of only reads UserPromptSubmit rows, so the PostToolUse event
    // produces no section and no title carries a tool name.
    let so = sections(&root, &db);
    assert!(so.contains("(2 rows)"), "only the two prompts section:\n{so}");
    assert!(!so.contains("PostToolUse"), "PostToolUse must not reach section_of:\n{so}");
}
