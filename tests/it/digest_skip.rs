//! Proof of the relation input-digest skip: a file edit that changes bytes but
//! not the extracted rows (a comment) rebuilds zero derived relations; an edit
//! that changes the rows (a new function) rebuilds the dependent relation. The
//! `_reldigest` baseline persists across the two `dl` processes (cold run then
//! `--changed`), which is the durable-state property the skip relies on.

use std::fs;
use std::path::{Path, PathBuf};
use std::process::Command;

const DL: &str = env!("CARGO_BIN_EXE_dl");

// A source relation `fn` extracted from .rs files, and a derived `allfn` over it.
const PROG: &str = r#"
rel fn(NAME: text, path: file).
rel allfn(name: text).
fn(NAME, path) <- scan("WORK", "src/**/*.rs", path, rev),
  sg(path, rev, :rust, "fn $NAME($$$P) { $$$B }", line).
allfn(name) <- fn(name, _).
? allfn(name).
"#;

const TWO_FNS: &str = "fn alpha() { let x = 1; }\nfn beta() { let y = 2; }\n";
// same two functions, a comment prepended: bytes differ, extracted rows identical.
const TWO_FNS_COMMENTED: &str =
    "// just a comment, same two fns\nfn alpha() { let x = 1; }\nfn beta() { let y = 2; }\n";
// a third function: the row set actually changes.
const THREE_FNS: &str =
    "fn alpha() { let x = 1; }\nfn beta() { let y = 2; }\nfn gamma() { let z = 3; }\n";

fn sandbox(tag: &str) -> PathBuf {
    let dir = std::env::temp_dir().join(format!("digest_skip_{tag}"));
    let _ = fs::remove_dir_all(&dir);
    fs::create_dir_all(dir.join("src")).unwrap();
    fs::write(dir.join("p.dl"), PROG).unwrap();
    dir
}

/// Cold full run that seeds `_reldigest`. Shares the db with later --changed runs.
fn cold(dir: &Path) {
    let st = Command::new(DL)
        .arg(dir.join("p.dl"))
        .args(["--root", dir.to_str().unwrap(), "--db", dir.join("db").to_str().unwrap()])
        .output().expect("cold run");
    assert!(st.status.success(), "cold run failed: {}", String::from_utf8_lossy(&st.stderr));
}

/// One incremental tick over the given path; returns the `[tick]` stderr line.
fn changed(dir: &Path, rel: &str) -> String {
    let out = Command::new(DL)
        .arg(dir.join("p.dl"))
        .args(["--root", dir.to_str().unwrap(), "--db", dir.join("db").to_str().unwrap(),
               "--changed", rel])
        .output().expect("changed run");
    String::from_utf8_lossy(&out.stderr).into_owned()
}

#[test]
fn comment_edit_rebuilds_nothing() {
    let d = sandbox("comment");
    fs::write(d.join("src/a.rs"), TWO_FNS).unwrap();
    cold(&d);
    fs::write(d.join("src/a.rs"), TWO_FNS_COMMENTED).unwrap();
    let line = changed(&d, "src/a.rs");
    assert!(line.contains("rebuilt derived: none"),
        "comment-only edit must rebuild nothing, got: {line}");
}

#[test]
fn real_edit_rebuilds_dependent() {
    let d = sandbox("real");
    fs::write(d.join("src/a.rs"), TWO_FNS).unwrap();
    cold(&d);
    fs::write(d.join("src/a.rs"), THREE_FNS).unwrap();
    let line = changed(&d, "src/a.rs");
    assert!(line.contains("rebuilt derived: allfn"),
        "a new function must rebuild the dependent relation, got: {line}");
}
