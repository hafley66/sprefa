//! Proof that the `--check` / `--diag-json` ban gate is a real gate: it fails on
//! a banned code move, passes on clean code, is STRUCTURAL (ignores the pattern
//! in comments/strings, unlike grep), reports the exact file:line, emits valid
//! JSON, and round-trips when the move is removed. Drives the built `dl` binary.

use std::fs;
use std::path::{Path, PathBuf};
use std::process::Command;

const DL: &str = env!("CARGO_BIN_EXE_dl");

const BAN_DL: &str = r#"
rel diag(path: file, line: int, severity: text, code: text, msg: text, hint: text).
diag(path, line, "error", "no-dbg", "dbg!() left in code", "remove it") <-
  scan("WORK", "v5/src/**/*.rs", path, rev), sg(path, rev, :rust, "dbg!($X)", line).
"#;

/// Fresh sandbox dir under the system temp dir, unique per test.
fn sandbox(tag: &str) -> PathBuf {
    let dir = std::env::temp_dir().join(format!("ban_gate_{tag}"));
    let _ = fs::remove_dir_all(&dir);
    fs::create_dir_all(dir.join("v5/src")).unwrap();
    fs::write(dir.join("ban.dl"), BAN_DL).unwrap();
    dir
}

fn write(root: &Path, rel: &str, body: &str) {
    fs::write(root.join(rel), body).unwrap();
}

/// Run `dl ban.dl --root <root> --db <fresh> <extra...>`; return (exit_code, stdout, stderr).
fn run(root: &Path, extra: &[&str]) -> (i32, String, String) {
    let db = root.join("p.db");
    let _ = fs::remove_file(&db); // fresh db each run, no stale state
    let out = Command::new(DL)
        .arg(root.join("ban.dl"))
        .args(["--root", root.to_str().unwrap(), "--db", db.to_str().unwrap()])
        .args(extra)
        .output()
        .expect("run dl");
    (
        out.status.code().unwrap_or(-1),
        String::from_utf8_lossy(&out.stdout).into_owned(),
        String::from_utf8_lossy(&out.stderr).into_owned(),
    )
}

#[test]
fn clean_code_passes() {
    let d = sandbox("clean");
    write(&d, "v5/src/clean.rs", "fn ok() { let x = 1; println!(\"{}\", x); }\n");
    let (code, _, _) = run(&d, &["--check"]);
    assert_eq!(code, 0, "clean tree must exit 0");
}

#[test]
fn structural_not_textual() {
    // `dbg!` appears in a comment AND a string literal but in no real call.
    let d = sandbox("structural");
    write(&d, "v5/src/decoy.rs",
        "// TODO: never use dbg!(x) in committed code\n\
         fn note() { let s = \"dbg!(y)\"; println!(\"{}\", s); }\n");
    let (code, _, err) = run(&d, &["--check"]);
    assert_eq!(code, 0, "dbg! in comment/string must NOT trip the gate (structural, not grep)");
    assert!(!err.contains("decoy.rs"), "no finding expected, got: {err}");
}

#[test]
fn real_call_fails_with_exact_location() {
    let d = sandbox("realcall");
    write(&d, "v5/src/bad.rs", "fn risky() {\n    let x = 7;\n    dbg!(x);\n}\n");
    // decoy in the same tree must stay silent
    write(&d, "v5/src/decoy.rs", "// dbg!(z) in a comment\n");
    let (code, _, err) = run(&d, &["--check"]);
    assert_eq!(code, 1, "a real dbg!() call must fail the gate");
    assert!(err.contains("v5/src/bad.rs:3"), "must point at bad.rs:3, got: {err}");
    assert!(!err.contains("decoy.rs"), "decoy must not be reported, got: {err}");
}

#[test]
fn diag_json_is_machine_checkable() {
    let d = sandbox("json");
    write(&d, "v5/src/bad.rs", "fn risky() {\n    let x = 7;\n    dbg!(x);\n}\n");
    let (_, out, _) = run(&d, &["--diag-json"]);
    // exactly one finding, at line 3, in bad.rs, severity error, code no-dbg
    let v: serde_json::Value = serde_json::from_str(&out).expect("valid JSON");
    let arr = v.as_array().expect("array");
    assert_eq!(arr.len(), 1, "exactly one finding");
    assert_eq!(arr[0]["line"], 3);
    assert_eq!(arr[0]["path"], "v5/src/bad.rs");
    assert_eq!(arr[0]["severity"], "error");
    assert_eq!(arr[0]["code"], "no-dbg");
}

#[test]
fn deterministic_round_trip() {
    let d = sandbox("roundtrip");
    write(&d, "v5/src/bad.rs", "fn risky() {\n    dbg!(7);\n}\n");
    let (code1, _, _) = run(&d, &["--check"]);
    assert_eq!(code1, 1, "with the move present, gate fails");
    fs::remove_file(d.join("v5/src/bad.rs")).unwrap();
    let (code2, _, _) = run(&d, &["--check"]);
    assert_eq!(code2, 0, "after removing the move, gate passes again");
}
