//! `scan(...)` outputs accept the `_` / omitted / kwarg forms, so authors stop
//! hand-counting positional slots. `scan("WORK", glob, p, _)` don't-cares the
//! rev; `scan(glob, p)` omits rev_out entirely; `scan(glob, path: p)` names it.

use std::fs;
use std::path::{Path, PathBuf};
use std::process::Command;

const DL: &str = env!("CARGO_BIN_EXE_dl");

fn sandbox(tag: &str) -> PathBuf {
    let dir = std::env::temp_dir().join(format!("scan_kwargs_{tag}"));
    let _ = fs::remove_dir_all(&dir);
    fs::create_dir_all(dir.join("src")).unwrap();
    fs::write(dir.join("src/a.rs"), "fn f() {}\n").unwrap();
    fs::write(dir.join("src/b.rs"), "fn g() {}\n").unwrap();
    dir
}

fn run(dir: &Path, prog: &str) -> (i32, String, String) {
    fs::write(dir.join("p.dl"), prog).unwrap();
    let out = Command::new(DL)
        .arg(dir.join("p.dl"))
        .args(["--db", dir.join("db").to_str().unwrap()])
        .current_dir(dir)
        .output().expect("run dl");
    (out.status.code().unwrap_or(-1),
     String::from_utf8_lossy(&out.stdout).into_owned(),
     String::from_utf8_lossy(&out.stderr).into_owned())
}

/// `scan("WORK", glob, p, _)` — the rev_out slot is a don't-care; `p` still binds
/// every matched file.
#[test]
fn wild_rev_out_binds_path() {
    let d = sandbox("wild");
    let (code, out, err) = run(&d, concat!(
        "rel f(p: file).\n",
        "f(p) <- scan(\"WORK\", \"src/**/*.rs\", p, _).\n",
        "? f(p).\n"));
    assert_eq!(code, 0, "{err}");
    assert!(out.contains("src/a.rs"), "path must bind:\n{out}");
    assert!(out.contains("src/b.rs"), "path must bind:\n{out}");
}

/// `scan(glob, p)` — rev_out omitted entirely (2-ary), repo=".", rev=WORK.
#[test]
fn omitted_rev_out_binds_path() {
    let d = sandbox("omit");
    let (code, out, err) = run(&d, concat!(
        "rel f(p: file).\n",
        "f(p) <- scan(\"src/**/*.rs\", p).\n",
        "? f(p).\n"));
    assert_eq!(code, 0, "{err}");
    assert!(out.contains("src/a.rs"), "{out}");
    assert!(out.contains("src/b.rs"), "{out}");
}

/// `scan(glob, path: p)` — named output, single positional input is the glob.
#[test]
fn named_path_output_binds_path() {
    let d = sandbox("named");
    let (code, out, err) = run(&d, concat!(
        "rel f(p: file).\n",
        "f(p) <- scan(\"src/**/*.rs\", path: p).\n",
        "? f(p).\n"));
    assert_eq!(code, 0, "{err}");
    assert!(out.contains("src/a.rs"), "{out}");
}

/// A named rev_out still binds the rev alongside a wild-friendly path slot.
#[test]
fn named_rev_out_binds_rev() {
    let d = sandbox("namedrev");
    let (code, out, err) = run(&d, concat!(
        "rel f(p: file, r: text).\n",
        "f(p, r) <- scan(\"WORK\", \"src/**/*.rs\", path: p, rev_out: r).\n",
        "? f(p, r).\n"));
    assert_eq!(code, 0, "{err}");
    assert!(out.contains("WORK"), "rev_out must bind WORK:\n{out}");
}
