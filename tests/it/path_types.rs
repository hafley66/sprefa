//! Typed path literals (spec T2): the six gate cases. A `fs:`/`glob:` literal
//! resolves at load time to canonical repo-relative text (fs) or a glob pattern,
//! joins existing path columns unchanged, and the type checker emits the spec's
//! diagnostics (path-escapes-root, brand-mismatch as errors; coerce as a warn).
//! Each case builds a temp repo, the same harness shape as tests/builtin_file_rel.rs.

use std::fs;
use std::path::{Path, PathBuf};
use std::process::Command;

const DL: &str = env!("CARGO_BIN_EXE_dl");

fn sandbox(tag: &str) -> PathBuf {
    let dir = std::env::temp_dir().join(format!("path_types_{tag}"));
    let _ = fs::remove_dir_all(&dir);
    fs::create_dir_all(&dir).unwrap();
    dir
}

fn run(dir: &Path, prog: &str, extra: &[&str]) -> (i32, String, String) {
    fs::write(dir.join("p.dl"), prog).unwrap();
    let out = Command::new(DL)
        .arg(dir.join("p.dl"))
        .args(["--db", dir.join("db").to_str().unwrap()])
        .args(extra)
        .current_dir(dir)
        .output()
        .expect("run dl");
    (
        out.status.code().unwrap_or(-1),
        String::from_utf8_lossy(&out.stdout).into_owned(),
        String::from_utf8_lossy(&out.stderr).into_owned(),
    )
}

/// (1) An `fs:` literal in a body atom resolves to canonical text and pins the
/// scanned path column, exactly like a quoted string would.
#[test]
fn fs_literal_joins_scanned_path() {
    let d = sandbox("fs_join");
    fs::create_dir_all(d.join("src")).unwrap();
    fs::write(d.join("src/x.rs"), "fn a() {}\n").unwrap();
    fs::write(d.join("src/y.rs"), "fn b() {}\n").unwrap();
    // The fs literal pins the scanned path column via a body atom: it is
    // immediately followed by `)`, the bare-form terminator (a trailing `.` would
    // otherwise be eaten as a path extension char).
    let prog = r#"
rel seen(path: file).
rel hit(path: file).
seen(p) <- scan("WORK", "src/**/*.rs", p, rev).
hit(fs:src/x.rs) <- seen(fs:src/x.rs).
? hit(p).
"#;
    let (code, out, err) = run(&d, prog, &[]);
    assert_eq!(code, 0, "fs literal join must succeed: {err}");
    assert!(
        out.contains("src/x.rs"),
        "fs:src/x.rs must pin the join: {out}"
    );
    assert!(
        out.contains("(1 rows)"),
        "exactly the one pinned file: {out}"
    );
    assert!(
        !out.contains("src/y.rs"),
        "the literal pins to x.rs only: {out}"
    );
}

/// (2) A `~/` anchor resolves to the scan root, so `fs:~/src/x.rs` is the same
/// canonical text as `fs:src/x.rs`.
#[test]
fn tilde_anchor_resolves_to_scan_root() {
    let d = sandbox("anchor");
    fs::create_dir_all(d.join("src")).unwrap();
    fs::write(d.join("src/x.rs"), "fn a() {}\n").unwrap();
    let prog = r#"
rel seen(path: file).
rel hit(path: file).
seen(p) <- scan("WORK", "src/**/*.rs", p, rev).
hit(fs:~/src/x.rs) <- seen(fs:~/src/x.rs).
? hit(p).
"#;
    let (code, out, err) = run(&d, prog, &[]);
    assert_eq!(code, 0, "~ anchor must resolve: {err}");
    assert!(
        out.contains("src/x.rs") && out.contains("(1 rows)"),
        "fs:~/src/x.rs must resolve to src/x.rs: {out}"
    );
}

/// (3) A concrete `fs:` literal whose normalized path climbs above the scan root
/// is a `path-escapes-root` error and fails `--check`.
#[test]
fn path_escapes_root_is_error() {
    let d = sandbox("escape");
    fs::create_dir_all(d.join("src")).unwrap();
    fs::write(d.join("src/x.rs"), "fn a() {}\n").unwrap();
    let prog = r#"
rel seen(path: file).
rel hit(path: file).
seen(p) <- scan("WORK", "src/**/*.rs", p, rev).
hit(fs:../outside.rs) <- seen(fs:../outside.rs).
? hit(p).
"#;
    let (code, _, err) = run(&d, prog, &["--check"]);
    assert_ne!(code, 0, "escaping the root must fail --check: {err}");
    assert!(
        err.contains("path-escapes-root"),
        "expected path-escapes-root code: {err}"
    );
}

/// (4) A variable bound at two columns with unrelated brands is a
/// `brand-mismatch` error.
#[test]
fn brand_mismatch_is_error() {
    let d = sandbox("brand");
    fs::write(d.join("a.txt"), "x\n").unwrap();
    let prog = r#"
type Sym <: text.
type Mod <: text.
rel a(s: Sym).
rel b(m: Mod).
rel bad(x: text).
a("foo") <- scan("WORK", "*.txt", f, rev), match(f, rev, /./, ln).
b("foo") <- scan("WORK", "*.txt", f, rev), match(f, rev, /./, ln).
bad(x) <- a(x), b(x).
? bad(x).
"#;
    let (code, _, err) = run(&d, prog, &["--check"]);
    assert_ne!(code, 0, "brand mismatch must fail --check: {err}");
    assert!(
        err.contains("brand-mismatch"),
        "expected brand-mismatch code: {err}"
    );
}

/// (5) A branded/path variable flowing into a plain `text` column is a coerce
/// WARNING: the program still compiles and runs (exit 0). The warning is noise on
/// a program that has accepted the coercion, so it is suppressed by default and
/// only printed under `DL_COERCE_WARN=1`.
#[test]
fn coerce_text_path_warns_but_runs() {
    let d = sandbox("coerce");
    fs::create_dir_all(d.join("src")).unwrap();
    fs::write(d.join("src/x.rs"), "fn a() {}\n").unwrap();
    let prog = r#"
rel seen(path: file).
rel note(x: text).
seen(p) <- scan("WORK", "src/**/*.rs", p, rev).
note(p) <- seen(p).
? note(p).
"#;
    // Default: the row still flows and the coerce warning is suppressed (no noise).
    let (code, out, err) = run(&d, prog, &[]);
    assert_eq!(code, 0, "coerce is a warning, run must succeed: {err}");
    assert!(out.contains("src/x.rs"), "the row still flows: {out}");
    assert!(
        !err.contains("coerce-text-path"),
        "coerce warning suppressed by default: {err}"
    );

    // Opt-in: `DL_COERCE_WARN=1` surfaces the warning (still a warn, not an error).
    fs::write(d.join("p.dl"), prog).unwrap();
    let out2 = Command::new(DL)
        .arg(d.join("p.dl"))
        .args(["--db", d.join("db2").to_str().unwrap()])
        .current_dir(d)
        .env("DL_COERCE_WARN", "1")
        .output()
        .expect("run dl");
    let err2 = String::from_utf8_lossy(&out2.stderr);
    assert_eq!(
        out2.status.code().unwrap_or(-1),
        0,
        "still runs under opt-in: {err2}"
    );
    assert!(
        err2.contains("coerce-text-path"),
        "coerce warning prints under DL_COERCE_WARN: {err2}"
    );
    assert!(
        !err2.contains("error[coerce"),
        "coerce must be a warn, not an error: {err2}"
    );
}

/// (6) `scan(.., glob:src/**/*.rs, ..)` is byte-identical to the quoted form
/// `scan(.., "src/**/*.rs", ..)`: the glob literal lowers exactly where a quoted
/// glob string lowers, so the scanned file set is the same.
#[test]
fn glob_literal_scan_byte_identical_to_quoted() {
    let d = sandbox("glob_scan");
    fs::create_dir_all(d.join("src")).unwrap();
    fs::write(d.join("src/x.rs"), "fn a() {}\n").unwrap();
    fs::write(d.join("src/y.rs"), "fn b() {}\n").unwrap();
    fs::write(d.join("top.rs"), "fn c() {}\n").unwrap();

    let quoted = r#"
rel seen(path: file).
seen(p) <- scan("WORK", "src/**/*.rs", p, rev).
? seen(p).
"#;
    let globbed = r#"
rel seen(path: file).
seen(p) <- scan("WORK", glob:src/**/*.rs, p, rev).
? seen(p).
"#;
    let dq = sandbox("glob_scan_q");
    for f in ["src/x.rs", "src/y.rs", "top.rs"] {
        let p = dq.join(f);
        fs::create_dir_all(p.parent().unwrap()).unwrap();
        fs::write(&p, "fn z() {}\n").unwrap();
    }
    let (cq, oq, _) = run(&dq, quoted, &[]);
    let (cg, og, eg) = run(&d, globbed, &[]);
    assert_eq!(cq, 0);
    assert_eq!(cg, 0, "glob literal scan must run: {eg}");
    assert_eq!(
        oq, og,
        "glob:src/**/*.rs must produce byte-identical output to the quoted form"
    );
    assert!(
        og.contains("src/x.rs") && og.contains("src/y.rs"),
        "src files matched: {og}"
    );
    assert!(!og.contains("top.rs"), "top.rs must not match src/**: {og}");
}
