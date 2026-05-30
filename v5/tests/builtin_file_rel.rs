//! Proof of the built-in `file` relation (data-model migration, Stage 1): any
//! rule can join the file set without a `scan`; the reserved names error if a
//! program redeclares them; and a cross-file reference validated by joining
//! `file` drops when the target file is deleted (FS-as-facts) — the primitive
//! the cross-codebase module graph is built on.

use std::fs;
use std::path::{Path, PathBuf};
use std::process::Command;

const DL: &str = env!("CARGO_BIN_EXE_dl");

fn sandbox(tag: &str) -> PathBuf {
    let dir = std::env::temp_dir().join(format!("builtin_file_{tag}"));
    let _ = fs::remove_dir_all(&dir);
    fs::create_dir_all(&dir).unwrap();
    dir
}

fn run(dir: &Path, prog: &str, extra: &[&str]) -> (i32, String, String) {
    fs::write(dir.join("p.dl"), prog).unwrap();
    let out = Command::new(DL)
        .arg(dir.join("p.dl"))
        .args(["--root", dir.to_str().unwrap(), "--db", dir.join("db").to_str().unwrap()])
        .args(extra)
        .output().expect("run dl");
    (out.status.code().unwrap_or(-1),
     String::from_utf8_lossy(&out.stdout).into_owned(),
     String::from_utf8_lossy(&out.stderr).into_owned())
}

#[test]
fn file_relation_is_queryable_without_scan() {
    let d = sandbox("queryable");
    fs::create_dir_all(d.join("src")).unwrap();
    fs::write(d.join("src/x.rs"), "fn a() {}\n").unwrap();
    fs::write(d.join("src/y.rs"), "fn b() {}\n").unwrap();
    // `seen` scans (populates the file set); `known` joins the built-in `file`
    // relation with no scan of its own.
    let prog = r#"
rel seen(path: file).
rel known(path: file).
seen(path) <- scan("WORK", "src/**/*.rs", path, rev), sg(path, rev, :rust, "fn $N() {}", line).
known(p) <- file(_, "WORK", p, _).
? known(p).
"#;
    let (code, out, _) = run(&d, prog, &[]);
    assert_eq!(code, 0);
    assert!(out.contains("src/x.rs") && out.contains("src/y.rs"), "file set must be queryable: {out}");
    assert!(out.contains("(2 rows)"), "expected both files: {out}");
}

#[test]
fn declaring_a_builtin_name_errors() {
    let d = sandbox("collision");
    fs::create_dir_all(d.join("src")).unwrap();
    fs::write(d.join("src/x.rs"), "fn a() {}\n").unwrap();
    let prog = "rel file(x: text).\nfile(a) <- scan(\"WORK\", \"src/**/*.rs\", a, r).\n";
    let (code, _, err) = run(&d, prog, &[]);
    assert_ne!(code, 0, "redeclaring `file` must fail");
    assert!(err.contains("built-in relation"), "expected built-in error, got: {err}");
}

#[test]
fn cross_file_ref_drops_when_target_deleted() {
    let d = sandbox("xref");
    fs::write(d.join("a.txt"), "ref b.txt\n").unwrap();
    fs::write(d.join("b.txt"), "hello\n").unwrap();
    // holds(a,b) only if `a` references `b` AND `b` is a real file (the join).
    let prog = r#"
rel ref(src: file, dst: text).
rel holds(src: file, dst: text).
ref(src, dst) <- scan("WORK", "*.txt", src, rev), match(src, rev, /ref (?<dst>\S+)/, line).
holds(src, dst) <- ref(src, dst), file(_, "WORK", dst, _).
? holds(src, dst).
"#;
    let (_, out, _) = run(&d, prog, &[]);
    assert!(out.contains("a.txt\tb.txt"), "cold: holds(a.txt,b.txt) expected: {out}");

    // Delete the target and tick only that path. a.txt is NOT reparsed, yet the
    // edge must drop because the `file` join no longer finds b.txt.
    fs::remove_file(d.join("b.txt")).unwrap();
    let (_, _, err) = run(&d, prog, &["--changed", "b.txt"]);
    assert!(err.contains("rebuilt derived: holds") || err.contains("holds"),
        "the holds rule (joins file) must rebuild on the file-set change: {err}");

    let (_, out2, _) = run(&d, prog, &[]);
    // holds is now empty: the reference survives but its target file is gone.
    assert!(out2.contains("(0 rows)"), "holds must drop when b.txt is gone: {out2}");
}
