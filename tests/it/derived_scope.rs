//! #13 write-storm fix: a program edit rebuilds ONLY the derived rels whose
//! rule shapes moved (per-rel `drv:` digests seeding the scoped arm), not the
//! whole derived layer. Before the fix every program edit wiped and rewrote
//! every derived table — GBs of byte-identical DELETE+INSERT on a real corpus.
//!
//! The discriminating probe is `DL_STMT_TRACE=1`: `rebuild_derived` names each
//! derived statement's rel before running it, so "was this rel's table touched"
//! is directly observable. Pre-fix, the program-edit run traces EVERY derived
//! rel; post-fix it traces only the edited subgraph.

use std::fs;
use std::path::{Path, PathBuf};
use std::process::Command;

const DL: &str = env!("CARGO_BIN_EXE_dl");

fn sandbox(tag: &str) -> PathBuf {
    let dir = std::env::temp_dir().join(format!("derived_scope_{tag}_{}", std::process::id()));
    let _ = fs::remove_dir_all(&dir);
    fs::create_dir_all(&dir).unwrap();
    dir
}

// Two independent derived chains over one scanned file: editing echo_b's rule
// must not touch echo_a's table.
const PROG_V1: &str = r#"
rel word_a(word: text).
word_a(word) <- scan("WORK", "f.txt", path, rev), match(path, rev, /a_(?<word>\w+)/, line).
rel word_b(word: text).
word_b(word) <- scan("WORK", "f.txt", path, rev), match(path, rev, /b_(?<word>\w+)/, line).
rel echo_a(word: text).
echo_a(word) <- word_a(word).
rel echo_b(word: text).
echo_b(word) <- word_b(word).
? echo_a(word).
? echo_b(word).
"#;

// Same program with echo_b's rule shape changed (output identical: the filter
// excludes nothing). Only echo_b's drv: digest moves.
const PROG_V2: &str = r#"
rel word_a(word: text).
word_a(word) <- scan("WORK", "f.txt", path, rev), match(path, rev, /a_(?<word>\w+)/, line).
rel word_b(word: text).
word_b(word) <- scan("WORK", "f.txt", path, rev), match(path, rev, /b_(?<word>\w+)/, line).
rel echo_a(word: text).
echo_a(word) <- word_a(word).
rel echo_b(word: text).
echo_b(word) <- word_b(word), word != "never_matches".
? echo_a(word).
? echo_b(word).
"#;

// echo_b's rule removed entirely (decl kept): the stored drv:echo_b key goes
// stale, the motion is unattributable, and the full rebuild must survive.
const PROG_V3: &str = r#"
rel word_a(word: text).
word_a(word) <- scan("WORK", "f.txt", path, rev), match(path, rev, /a_(?<word>\w+)/, line).
rel word_b(word: text).
word_b(word) <- scan("WORK", "f.txt", path, rev), match(path, rev, /b_(?<word>\w+)/, line).
rel echo_a(word: text).
echo_a(word) <- word_a(word).
rel echo_b(word: text).
? echo_a(word).
"#;

fn run(dir: &Path, prog: &str) -> (i32, String, String) {
    fs::write(dir.join("p.dl"), prog).unwrap();
    let out = Command::new(DL)
        .arg(dir.join("p.dl"))
        .args(["--no-daemon", "--db", dir.join("db").to_str().unwrap()])
        .current_dir(dir)
        .env("SPREFA_CONFIG", "/dev/null")
        .env("DL_STMT_TRACE", "1")
        .output()
        .expect("run dl");
    (
        out.status.code().unwrap_or(-1),
        String::from_utf8_lossy(&out.stdout).into_owned(),
        String::from_utf8_lossy(&out.stderr).into_owned(),
    )
}

#[test]
fn program_edit_rebuilds_only_the_edited_subgraph() {
    let dir = sandbox("scoped");
    fs::write(dir.join("f.txt"), "a_alpha b_beta\n").unwrap();

    let (code1, out1, err1) = run(&dir, PROG_V1);
    assert_eq!(code1, 0, "v1 run failed: {err1}");
    assert!(out1.contains("alpha") && out1.contains("beta"), "v1 rows:\n{out1}");
    assert!(err1.contains("[stmt-trace] echo_a") && err1.contains("[stmt-trace] echo_b"),
        "blank slate builds both chains:\n{err1}");

    let (code2, out2, err2) = run(&dir, PROG_V2);
    assert_eq!(code2, 0, "v2 run failed: {err2}");
    assert!(err2.contains("[derived-scope] program edit scoped to"),
        "the edit must downgrade to the scoped arm:\n{err2}");
    assert!(err2.contains("[stmt-trace] echo_b"),
        "the edited rel rebuilds:\n{err2}");
    assert!(!err2.contains("[stmt-trace] echo_a"),
        "the untouched chain must not be wiped or rewritten (the #13 storm):\n{err2}");
    // Correctness: the untouched table still serves its rows, the edited one
    // recomputed to the same output.
    assert!(out2.contains("alpha"), "echo_a rows intact:\n{out2}");
    assert!(out2.contains("beta"), "echo_b recomputed:\n{out2}");
}

#[test]
fn unattributable_program_edit_keeps_the_full_rebuild() {
    let dir = sandbox("fallback");
    fs::write(dir.join("f.txt"), "a_alpha b_beta\n").unwrap();

    let (code1, _out1, err1) = run(&dir, PROG_V2);
    assert_eq!(code1, 0, "seed run failed: {err1}");

    // All of echo_b's rules removed: drv:echo_b goes stale, no current head
    // owns the motion, so the conservative full rebuild runs.
    let (code2, out2, err2) = run(&dir, PROG_V3);
    assert_eq!(code2, 0, "v3 run failed: {err2}");
    assert!(!err2.contains("[derived-scope]"),
        "a stale drv: key is unattributable; no scoping:\n{err2}");
    assert!(err2.contains("[stmt-trace] echo_a"),
        "full rebuild reaches the surviving chain:\n{err2}");
    assert!(out2.contains("alpha"), "echo_a rows survive the fallback:\n{out2}");
}
