//! The built-in `dl_diag` self-validation relation: the engine runs its own
//! lexer/parser/typechecker over every scanned `.dl` file and emits one row per
//! diagnostic (path, line, col, end_line, end_col, severity, code, msg) — the
//! same pass as `dl --check`, relocated into a relation so a `.dl` program can
//! lint `.dl`. Git-free: it reads the on-disk working copy by path, no repo.

use std::fs;
use std::path::{Path, PathBuf};
use std::process::Command;

const DL: &str = env!("CARGO_BIN_EXE_dl");

fn sandbox(tag: &str) -> PathBuf {
    let dir = std::env::temp_dir().join(format!("dl_diag_{tag}"));
    let _ = fs::remove_dir_all(&dir);
    fs::create_dir_all(&dir).unwrap();
    dir
}

/// Run a probe program (written to `probe.dl`) that scans every `.dl` and queries
/// `dl_diag`. Returns (exit code, stdout, stderr).
fn run(dir: &Path, prog: &str, extra: &[&str]) -> (i32, String, String) {
    fs::write(dir.join("probe.dl"), prog).unwrap();
    let out = Command::new(DL)
        .arg(dir.join("probe.dl"))
        .args(["--db", dir.join("db").to_str().unwrap()])
        .current_dir(dir)
        .args(extra)
        .output()
        .expect("run dl");
    (
        out.status.code().unwrap_or(-1),
        String::from_utf8_lossy(&out.stdout).into_owned(),
        String::from_utf8_lossy(&out.stderr).into_owned(),
    )
}

const PROBE: &str = concat!(
    "rel dl_file(p: file).\n",
    "dl_file(p) <- scan(\"**/*.dl\", p, rev).\n",
    "? dl_diag(path, line, col, end_line, end_col, severity, code, msg).\n",
);

/// (1) A syntactically broken `.dl` file is flagged with a parse diagnostic; a
/// valid sibling is not. No git in the sandbox — proves the relation is git-free.
#[test]
fn flags_broken_dl_no_git() {
    let d = sandbox("broken");
    // valid program
    fs::write(d.join("ok.dl"), "rel a(x: text).\na(x) <- b(x).\n").unwrap();
    // missing comma between body atoms => parse error
    fs::write(
        d.join("bad.dl"),
        "rel t(p: file, n: int).\nt(p, n) <- scan(\"*.dl\", p, rev) n = 1.\n",
    )
    .unwrap();
    let (code, out, err) = run(&d, PROBE, &[]);
    assert_eq!(code, 0, "stderr: {err}");
    assert!(
        out.contains("bad.dl"),
        "broken file must be flagged:\n{out}"
    );
    assert!(out.contains("parse"), "parse code expected:\n{out}");
    assert!(
        !out.contains("\tok.dl\t") && !out.lines().any(|l| l.starts_with("ok.dl\t")),
        "valid file must not be flagged:\n{out}"
    );
}

/// (2) A type error (aggregation produces int into a text column) surfaces with
/// the engine's typecheck code, matching `dl --check`.
#[test]
fn flags_type_error() {
    let d = sandbox("typed");
    fs::write(d.join("typed.dl"),
        "rel edge(f: text, t: text).\nrel fan(f: text, n: text).\nfan(f, count(t)) <- edge(f, t).\n").unwrap();
    let (code, out, err) = run(&d, PROBE, &[]);
    assert_eq!(code, 0, "stderr: {err}");
    assert!(
        out.contains("typed.dl"),
        "typed file must be flagged:\n{out}"
    );
    assert!(
        out.contains("brand-mismatch"),
        "typecheck code expected:\n{out}"
    );
}

/// (3) The rail shape: `diag <- dl_diag(...)` makes a broken `.dl` a blocking
/// `--check` failure (exit 2), the rust-analyzer-on-save behavior for `.dl`.
#[test]
fn rail_blocks_on_broken_dl_under_check() {
    let d = sandbox("rail");
    fs::write(
        d.join("bad.dl"),
        "rel t(p: file).\nt(p) <- scan(\"*.dl\" p, rev).\n",
    )
    .unwrap();
    let prog = concat!(
        "rel dl_file(p: file).\n",
        "dl_file(p) <- scan(\"**/*.dl\", p, rev).\n",
        "diag(path: p, line, col, end_line, end_col, severity: \"error\", code, msg) <- ",
        "dl_file(p), p =~ /bad\\.dl$/, dl_diag(p, line, col, end_line, end_col, _, code, msg).\n",
    );
    let (code, _out, _err) = run(&d, prog, &["--check"]);
    assert_eq!(code, 2, "a broken .dl must block --check (exit 2)");
}

/// (4) `dl_diag` is a reserved name.
#[test]
fn dl_diag_is_reserved() {
    let d = sandbox("reserved");
    let (code, _out, err) = run(&d, "rel dl_diag(p: text).\n", &[]);
    assert_ne!(code, 0);
    assert!(
        err.contains("built-in") || err.contains("dl self-diagnostics"),
        "reserved-name error expected:\n{err}"
    );
}

/// (new) The unpinned-closure-query lint: a `?` on a closure head with both
/// endpoints free warns (code closure-unpinned, with the pin hint) — the lint
/// twin of the runtime DL_CLOSURE_QUERY_MAX_EDGES guard. A pinned query on the
/// same head is silent.
#[test]
fn warns_on_unpinned_closure_query() {
    let d = sandbox("closure_unpinned");
    fs::write(
        d.join("walk.dl"),
        "rel e(a: text, b: text).\ne(\"x\", \"y\").\n\
         rel reach(from: text, to: text).\nreach(a, b) <- closure(e).\n\
         ? reach(from, to).\n",
    )
    .unwrap();
    fs::write(
        d.join("pinned.dl"),
        "rel e2(a: text, b: text).\ne2(\"x\", \"y\").\n\
         rel reach2(from: text, to: text).\nreach2(a, b) <- closure(e2).\n\
         ? reach2(\"x\", to).\n",
    )
    .unwrap();
    let (code, out, err) = run(&d, PROBE, &[]);
    assert_eq!(code, 0, "stderr: {err}");
    assert!(
        out.contains("closure-unpinned") && out.contains("walk.dl"),
        "unpinned query flagged:\n{out}"
    );
    assert!(!out.contains("pinned.dl\t"), "pinned query silent:\n{out}");
}
