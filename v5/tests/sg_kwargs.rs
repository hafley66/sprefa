//! `sg(...)` span outputs accept the kwarg / wildcard / omitted forms, mirroring
//! `comment()`. `sg(p, rev, :rust, "pat", end_col: ec)` binds only end_col;
//! `sg(p, rev, :rust, "pat", _, _, _, ec)` drops the prefix; a zero-output call
//! is a file-existence filter on the pattern.

use std::fs;
use std::path::{Path, PathBuf};
use std::process::Command;

const DL: &str = env!("CARGO_BIN_EXE_dl");

fn sandbox(tag: &str) -> PathBuf {
    let dir = std::env::temp_dir().join(format!("sg_kwargs_{tag}"));
    let _ = fs::remove_dir_all(&dir);
    fs::create_dir_all(dir.join("src")).unwrap();
    fs::write(dir.join("src/a.rs"), "fn f() {\n    dbg!(x);\n    dbg!(y);\n}\n").unwrap();
    dir
}

fn run(dir: &Path, prog: &str) -> (i32, String, String) {
    fs::write(dir.join("p.dl"), prog).unwrap();
    let out = Command::new(DL)
        .arg(dir.join("p.dl"))
        .args(["--root", dir.to_str().unwrap(), "--db", dir.join("db").to_str().unwrap()])
        .output().expect("run dl");
    (out.status.code().unwrap_or(-1),
     String::from_utf8_lossy(&out.stdout).into_owned(),
     String::from_utf8_lossy(&out.stderr).into_owned())
}

/// `sg(p, rev, :rust, "dbg!($X)", end_col: ec)` — kwarg binds only end_col,
/// skipping line/col/end_line without dummy vars.
#[test]
fn kwarg_binds_only_the_named_span_output() {
    let d = sandbox("kwarg");
    let (code, out, err) = run(&d, concat!(
        "rel hit(p: file, ec: int).\n",
        "hit(p, ec) <- scan(\"WORK\", \"src/**/*.rs\", p, rev), ",
        "sg(p, rev, :rust, \"dbg!($X)\", end_col: ec).\n",
        "? hit(p, ec).\n"));
    assert_eq!(code, 0, "{err}");
    // `dbg!(x)` spans bytes 4..11; `dbg!(y)` also 4..11 (same indent). Set dedup.
    assert!(out.contains("\t11"), "end_col kwarg must bind 11:\n{out}");
}

/// `sg(p, rev, :rust, "dbg!($X)", _, _, _, ec)` — wildcard prefix.
#[test]
fn wildcard_outputs_drop_the_prefix_slots() {
    let d = sandbox("wild");
    let (code, out, err) = run(&d, concat!(
        "rel hit(p: file, ec: int).\n",
        "hit(p, ec) <- scan(\"WORK\", \"src/**/*.rs\", p, rev), ",
        "sg(p, rev, :rust, \"dbg!($X)\", _, _, _, ec).\n",
        "? hit(p, ec).\n"));
    assert_eq!(code, 0, "{err}");
    assert!(out.contains("\t11"), "{out}");
}

/// `sg(p, rev, :rust, "dbg!($X)", line, end_col: ec)` — positional line then
/// kwarg end_col skips col/end_line in the middle.
#[test]
fn mixed_positional_then_kwarg_skips_middle() {
    let d = sandbox("mixed");
    let (code, out, err) = run(&d, concat!(
        "rel hit(p: file, l: int, ec: int).\n",
        "hit(p, l, ec) <- scan(\"WORK\", \"src/**/*.rs\", p, rev), ",
        "sg(p, rev, :rust, \"dbg!($X)\", l, end_col: ec).\n",
        "? hit(p, l, ec).\n"));
    assert_eq!(code, 0, "{err}");
    assert!(out.contains("\t2\t11"), "line 2 end_col 11:\n{out}");
    assert!(out.contains("\t3\t11"), "line 3 end_col 11:\n{out}");
}

/// `sg(p, rev, :rust, "dbg!($X)")` — zero span outputs: a file filter that
/// emits one row per matching file (set-deduped, one per file regardless of
/// match count).
#[test]
fn zero_outputs_is_a_file_existence_filter() {
    let d = sandbox("zero");
    let (code, out, err) = run(&d, concat!(
        "rel hit(p: file).\n",
        "hit(p) <- scan(\"WORK\", \"src/**/*.rs\", p, rev), ",
        "sg(p, rev, :rust, \"dbg!($X)\").\n",
        "? hit(p).\n"));
    assert_eq!(code, 0, "{err}");
    // One file matches; two matches in it dedup to one row.
    let rows: Vec<&str> = out.lines().filter(|l| l.contains("a.rs")).collect();
    assert_eq!(rows.len(), 1, "one row per file (set dedup), got {rows:?}:\n{out}");
}

/// A typo'd span name is a parse error at parse time.
#[test]
fn unknown_kwarg_name_is_a_parse_error() {
    let d = sandbox("bad");
    let (code, _out, err) = run(&d, concat!(
        "rel x(p: file).\n",
        "x(p) <- scan(\"WORK\", \"src/**/*.rs\", p, rev), ",
        "sg(p, rev, :rust, \"dbg!($X)\", bogus: v).\n"));
    assert_ne!(code, 0, "expected parse failure");
    assert!(err.contains("unknown output arg"), "expected unknown-arg diagnostic:\n{err}");
}
