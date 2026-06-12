//! Int arithmetic in rule heads and comparisons: `r(p, n + 1) <- ...`,
//! `n * 2 > 4`. Allowed in derived heads (lowers to SQL arithmetic), source
//! heads (evaluated on the bound row), and either side of a comparison. Body
//! atom positions stay binding-only (parse/lower error). `/` after a value is
//! division; after a delimiter it still opens a /regex/.

use std::fs;
use std::path::{Path, PathBuf};
use std::process::Command;

const DL: &str = env!("CARGO_BIN_EXE_dl");

fn sandbox(tag: &str) -> PathBuf {
    let dir = std::env::temp_dir().join(format!("arith_{tag}"));
    let _ = fs::remove_dir_all(&dir);
    fs::create_dir_all(dir.join("src")).unwrap();
    fs::write(dir.join("src/x.rs"), "fn alpha() {}\nfn beta() {}\nfn gamma() {}\n").unwrap();
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

/// Derived-head arithmetic lowers to SQL: line 1/2/3 -> rank 2/3/4, doubled 2/4/6.
#[test]
fn derived_head_arithmetic() {
    let d = sandbox("derived");
    let prog = r#"
rel fns(p: file, line: int).
fns(p, line) <- scan("WORK", "src/*.rs", p, rev), match(p, rev, /fn /, line).
rel rank(p: file, r: int).
rank(p, line + 1) <- fns(p, line).
rel dbl(p: file, r: int).
dbl(p, line * 2) <- fns(p, line).
? rank(p, r).
? dbl(p, r).
"#;
    let (code, out, err) = run(&d, prog);
    assert_eq!(code, 0, "{err}");
    let rank = out.split("? rank =>").nth(1).unwrap().split('?').next().unwrap();
    for n in ["\t2", "\t3", "\t4"] { assert!(rank.contains(n), "rank rows: {rank}"); }
    let dbl = out.split("? dbl =>").nth(1).unwrap();
    for n in ["\t2", "\t4", "\t6"] { assert!(dbl.contains(n), "dbl rows: {dbl}"); }
}

/// Precedence and parens: `1 + line * 2` and `(line + 1) * 2` differ.
#[test]
fn precedence_and_parens() {
    let d = sandbox("prec");
    let prog = r#"
rel fns(p: file, line: int).
fns(p, line) <- scan("WORK", "src/*.rs", p, rev), match(p, rev, /fn /, line).
rel a(p: file, v: int).
a(p, 1 + line * 2) <- fns(p, line).
rel b(p: file, v: int).
b(p, (line + 1) * 2) <- fns(p, line).
? a(p, v).
? b(p, v).
"#;
    let (code, out, err) = run(&d, prog);
    assert_eq!(code, 0, "{err}");
    // line 1: a = 1 + 1*2 = 3; b = (1+1)*2 = 4
    let a = out.split("? a =>").nth(1).unwrap().split('?').next().unwrap();
    assert!(a.contains("\t3"), "a rows: {a}");
    let b = out.split("? b =>").nth(1).unwrap();
    assert!(b.contains("\t4"), "b rows: {b}");
}

/// Source-head arithmetic is evaluated on the bound row (no SQL involved):
/// the head row carries line - 1 directly out of extraction.
#[test]
fn source_head_arithmetic() {
    let d = sandbox("source");
    let prog = r#"
rel above(p: file, prev: int).
above(p, line - 1) <- scan("WORK", "src/*.rs", p, rev), match(p, rev, /fn /, line).
? above(p, prev).
"#;
    let (code, out, err) = run(&d, prog);
    assert_eq!(code, 0, "{err}");
    for n in ["\t0", "\t1", "\t2"] { assert!(out.contains(n), "{out}"); }
}

/// Arithmetic on a comparison side, including `/` as division after a value.
#[test]
fn comparison_arithmetic_and_division() {
    let d = sandbox("cmp");
    let prog = r#"
rel fns(p: file, line: int).
fns(p, line) <- scan("WORK", "src/*.rs", p, rev), match(p, rev, /fn /, line).
rel big(p: file, line: int).
big(p, line) <- fns(p, line), line * 2 > 4.
rel half(p: file, h: int).
half(p, line / 2) <- fns(p, line), line % 2 = 0.
? big(p, line).
? half(p, h).
"#;
    let (code, out, err) = run(&d, prog);
    assert_eq!(code, 0, "{err}");
    let big = out.split("? big =>").nth(1).unwrap().split('?').next().unwrap();
    assert!(big.contains("\t3") && !big.contains("\t1") && !big.contains("\t2"), "big rows: {big}");
    let half = out.split("? half =>").nth(1).unwrap();
    assert!(half.contains("\t1"), "half of line 2: {half}");
}

/// A regex literal after `,` still lexes as a regex (the `/`-division heuristic
/// only fires after a value token) — pinned by every other test using match(),
/// plus an explicit body-position error here: arithmetic in a body atom.
#[test]
fn arithmetic_in_body_atom_is_an_error() {
    let d = sandbox("body");
    let prog = r#"
rel fns(p: file, line: int).
fns(p, line) <- scan("WORK", "src/*.rs", p, rev), match(p, rev, /fn /, line).
rel r(p: file).
r(p) <- fns(p, line + 1).
? r(p).
"#;
    let (code, _out, err) = run(&d, prog);
    assert_ne!(code, 0, "arithmetic in a body atom must fail");
    assert!(err.contains("expected , or ) in atom, got Plus"), "{err}");
}

/// Arithmetic into a non-int column is a typecheck error, not a SQLite crash.
#[test]
fn arithmetic_into_text_column_errors() {
    let d = sandbox("ty");
    let prog = r#"
rel fns(p: file, line: int).
fns(p, line) <- scan("WORK", "src/*.rs", p, rev), match(p, rev, /fn /, line).
rel r(p: file, v: text).
r(p, line + 1) <- fns(p, line).
? r(p, v).
"#;
    let (code, _out, err) = run(&d, prog);
    assert_ne!(code, 0, "arith into text column must fail");
    assert!(err.contains("brand-mismatch"), "{err}");
    assert!(err.contains("non-int column"), "{err}");
}
