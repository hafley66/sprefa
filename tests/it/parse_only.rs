//! `--parse-only`: parse + typecheck + op resolution + metavar sanity over a
//! program with NO scan and NO db writes. Exit 0 clean (warns are fine), 1 on
//! any error-severity diagnostic. Drives the real binary with `--no-daemon`.

use std::fs;
use std::path::{Path, PathBuf};
use std::process::Command;

const DL: &str = env!("CARGO_BIN_EXE_dl");

fn sandbox(tag: &str) -> PathBuf {
    let dir = std::env::temp_dir().join(format!("parseonly_{tag}_{}", std::process::id()));
    let _ = fs::remove_dir_all(&dir);
    fs::create_dir_all(dir.join("src")).unwrap();
    // A real source file exists so a scan WOULD have work to do — proving the
    // no-scan promise is about the flag, not an empty tree.
    fs::write(dir.join("src/x.rs"), "fn alpha() {}\n").unwrap();
    dir
}

fn run(dir: &Path, prog: &str, db: &Path) -> (i32, String, String) {
    fs::write(dir.join("p.dl"), prog).unwrap();
    let out = Command::new(DL)
        .arg(dir.join("p.dl"))
        .args(["--no-daemon",
               "--parse-only", "--db", db.to_str().unwrap()])
        .current_dir(dir)
        .output().expect("run dl --parse-only");
    (out.status.code().unwrap_or(-1),
     String::from_utf8_lossy(&out.stdout).into_owned(),
     String::from_utf8_lossy(&out.stderr).into_owned())
}

/// A clean program parses to exit 0 and writes NO db file (proving no scan/tick
/// ran — a real run would have opened the db passed via `--db`).
#[test]
fn clean_program_exits_zero_and_writes_no_db() {
    let d = sandbox("clean");
    let db = d.join("should_not_exist.db");
    let (code, _out, err) = run(&d, concat!(
        "rel hit(p: file, l: int).\n",
        "hit(p, l) <- scan(\"WORK\", \"src/**/*.rs\", p, rev), match(p, rev, /alpha/, l).\n",
        "? hit(p, l).\n",
    ), &db);
    assert_eq!(code, 0, "clean program exits 0:\n{err}");
    assert!(!db.exists(), "no db file created (no scan/tick ran): {}", db.display());
    let _ = fs::remove_dir_all(&d);
}

/// A parse error exits 1 (diagnostics to stderr), still with no db written.
#[test]
fn parse_error_exits_one() {
    let d = sandbox("parseerr");
    let db = d.join("should_not_exist.db");
    // Missing closing paren / broken head: a hard parse failure.
    let (code, _out, err) = run(&d, "rel hit(p: file, l: int.\n", &db);
    assert_eq!(code, 1, "a parse error is exit 1:\n{err}");
    assert!(!db.exists(), "no db file created on parse error");
    let _ = fs::remove_dir_all(&d);
}

/// A lowercase `$name` in an sg pattern warns (not errors) under `--parse-only`;
/// warn-only keeps the exit at 0, and the diagnostic names the fix.
#[test]
fn lowercase_metavar_warns_but_exits_zero() {
    let d = sandbox("lcwarn");
    let db = d.join("should_not_exist.db");
    let (code, _out, err) = run(&d, concat!(
        "rel body_hit(l: int).\n",
        "body_hit(l) <- scan(\"WORK\", \"src/**/*.rs\", p, rev), ",
        "sg(p, rev, :rust, \"fn $name() {}\", l).\n",
    ), &db);
    assert!(err.contains("lowercase-metavar"), "warn code surfaces:\n{err}");
    assert!(err.contains("metavars are UPPERCASE"), "warn names the fix:\n{err}");
    assert_eq!(code, 0, "a warn-only program still exits 0:\n{err}");
    assert!(!db.exists(), "no db file created");
    let _ = fs::remove_dir_all(&d);
}

/// A lookahead regex in a `match` op is unsupported by the Rust regex crate.
/// `--parse-only` compiles every literal, so it fails HERE (exit 1) instead of
/// surviving to a real scan; the message carries both the regex-crate caret
/// error and the dl escape note.
#[test]
fn match_lookahead_regex_exits_one_with_note() {
    let d = sandbox("relookahead");
    let db = d.join("should_not_exist.db");
    let (code, _out, err) = run(&d, concat!(
        "rel banned(p: file, l: int).\n",
        "banned(p, l) <- scan(\"WORK\", \"src/**/*.rs\", p, rev), ",
        "match(p, rev, /(?!-)prefix/, l).\n",
    ), &db);
    assert_eq!(code, 1, "an uncompilable regex is exit 1:\n{err}");
    assert!(err.contains("look-around"), "raw regex-crate error surfaces:\n{err}");
    assert!(err.contains("regexes are Rust-flavor"), "the dl escape note is appended:\n{err}");
    assert!(!db.exists(), "no db file created on a regex error");
    let _ = fs::remove_dir_all(&d);
}

/// Same rejection for an `=~` body constraint regex: the literal is compiled at
/// parse-only time, so a look-behind fails with the caret error plus the note.
#[test]
fn constraint_lookbehind_regex_exits_one_with_note() {
    let d = sandbox("recons");
    let db = d.join("should_not_exist.db");
    let (code, _out, err) = run(&d, concat!(
        "rel named(p: file, l: int).\n",
        "named(p, l) <- scan(\"WORK\", \"src/**/*.rs\", p, rev), ",
        "match(p, rev, /fn/, l), p =~ /(?<=x)y/.\n",
    ), &db);
    assert_eq!(code, 1, "an uncompilable =~ regex is exit 1:\n{err}");
    assert!(err.contains("look-around"), "raw regex-crate error surfaces:\n{err}");
    assert!(err.contains("regexes are Rust-flavor"), "the dl escape note is appended:\n{err}");
    assert!(!db.exists(), "no db file created on a regex error");
    let _ = fs::remove_dir_all(&d);
}

/// A program whose every regex literal compiles exits 0 (the anchored form the
/// note recommends is the fix for the two rejected cases above).
#[test]
fn valid_regex_literals_exit_zero() {
    let d = sandbox("reok");
    let db = d.join("should_not_exist.db");
    let (code, _out, err) = run(&d, concat!(
        "rel anchored(p: file, l: int).\n",
        "anchored(p, l) <- scan(\"WORK\", \"src/**/*.rs\", p, rev), ",
        "match(p, rev, /\\bprefix\\b/, l), p =~ /x$/.\n",
    ), &db);
    assert_eq!(code, 0, "valid regex literals exit 0:\n{err}");
    assert!(!db.exists(), "no db file created (no scan/tick ran)");
    let _ = fs::remove_dir_all(&d);
}
