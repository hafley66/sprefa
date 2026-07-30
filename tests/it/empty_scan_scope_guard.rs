//! Empty-scan-scope guard: a program with no scan rules ticked against a db that
//! already has file-scoped source rows must not reconcile those rows away.
//! Derived rules over the existing rows still evaluate normally.
//!
//! Regression for the "zero scan atoms wipes the db" hazard: the engine must
//! distinguish "no scan rules in this program" from "scan rules matched zero
//! files". Only the former is guarded.

use std::fs;
use std::path::{Path, PathBuf};
use std::process::Command;

const DL: &str = env!("CARGO_BIN_EXE_dl");

fn sandbox(tag: &str) -> PathBuf {
    let dir = std::env::temp_dir().join(format!("dl_empty_scan_guard_{tag}"));
    let _ = fs::remove_dir_all(&dir);
    fs::create_dir_all(dir.join("src")).unwrap();
    fs::write(dir.join("src/a.rs"), "fn f() {}\n").unwrap();
    fs::write(dir.join("src/b.rs"), "fn g() {}\n").unwrap();
    dir
}

fn run(dir: &Path, prog: &str, db: &str) -> (i32, String, String) {
    fs::write(dir.join("p.dl"), prog).unwrap();
    let out = Command::new(DL)
        .arg(dir.join("p.dl"))
        .arg("--no-daemon")
        .args(["--db", dir.join(db).to_str().unwrap()])
        .current_dir(dir)
        .output()
        .expect("run dl");
    (
        out.status.code().unwrap_or(-1),
        String::from_utf8_lossy(&out.stdout).into_owned(),
        String::from_utf8_lossy(&out.stderr).into_owned(),
    )
}

/// A no-scan-rules program run against a db with prior scanned files must leave
/// the file set intact and still let derived rules read existing facts.
#[test]
fn no_scan_rules_preserves_existing_files() {
    let d = sandbox("preserve");

    let prog1 = concat!(
        "rel seen(path: file).\n",
        "seen(p) <- scan(\"WORK\", \"src/**/*.rs\", p, rev).\n",
        "? seen(p).\n"
    );
    let (code, out, err) = run(&d, prog1, "db");
    assert_eq!(code, 0, "{err}");
    assert!(
        out.contains("src/a.rs"),
        "first tick must scan a.rs:\n{out}"
    );
    assert!(
        out.contains("src/b.rs"),
        "first tick must scan b.rs:\n{out}"
    );

    // Second program has NO scan rules. It derives over the persisted `seen`.
    let prog2 = concat!(
        "rel seen(path: file).\n",
        "rel total(n: int).\n",
        "total(count(p)) <- seen(p).\n",
        "? file(repo, rev, path, content).\n",
        "? total(n).\n"
    );
    let (code2, out2, err2) = run(&d, prog2, "db");
    assert_eq!(code2, 0, "{err2}");
    assert!(
        err2.contains("program has no scan rules; leaving 2 existing files untouched"),
        "expected guard stderr, got:\n{err2}"
    );
    assert!(
        out2.contains("src/a.rs") && out2.contains("src/b.rs"),
        "file set must be preserved:\n{out2}"
    );
    assert!(out2.contains("2"), "derived total must be 2:\n{out2}");
}

/// Scan rules that match zero files (but scan rules exist) still reconcile down
/// honestly — the guard does NOT apply here.
#[test]
fn zero_match_scan_still_reconciles() {
    let d = sandbox("zeromatch");

    let prog1 = concat!(
        "rel seen(path: file).\n",
        "seen(p) <- scan(\"WORK\", \"src/**/*.rs\", p, rev).\n",
        "? seen(p).\n"
    );
    let (code, out, err) = run(&d, prog1, "db");
    assert_eq!(code, 0, "{err}");
    assert!(out.contains("src/a.rs"), "{out}");

    let prog2 = concat!(
        "rel seen(path: file).\n",
        "seen(p) <- scan(\"WORK\", \"no_such_dir/**/*.rs\", p, rev).\n",
        "? seen(p).\n"
    );
    let (code2, out2, err2) = run(&d, prog2, "db");
    assert_eq!(code2, 0, "{err2}");
    assert!(
        !out2.contains("src/a.rs"),
        "matched-zero scan must reconcile away:\n{out2}"
    );
    assert!(
        !err2.contains("program has no scan rules"),
        "guard must not fire when scan rules exist:\n{err2}"
    );
}
