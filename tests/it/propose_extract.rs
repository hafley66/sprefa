//! The built-in `propose_extract(path, lo, hi, param)` relation: for every
//! scanned Rust file, the verbatim clone-detection kernel finds duplicated
//! line-blocks and infers the extract-fn signature. One row per (block, param).

use std::fs;
use std::path::{Path, PathBuf};
use std::process::Command;

const DL: &str = env!("CARGO_BIN_EXE_dl");

fn sandbox(tag: &str) -> PathBuf {
    let dir = std::env::temp_dir().join(format!("propose_{tag}"));
    let _ = fs::remove_dir_all(&dir);
    fs::create_dir_all(&dir).unwrap();
    dir
}

fn run(dir: &Path, prog: &str) -> (i32, String, String) {
    fs::write(dir.join("p.dl"), prog).unwrap();
    let out = Command::new(DL)
        .arg(dir.join("p.dl"))
        .current_dir(dir)
        .args(["--db", dir.join("db").to_str().unwrap()])
        .output().expect("run dl");
    (out.status.code().unwrap_or(-1),
     String::from_utf8_lossy(&out.stdout).into_owned(),
     String::from_utf8_lossy(&out.stderr).into_owned())
}

/// (1) Two functions with 6 identical body lines: the verbatim kernel (seed=6)
/// finds the dup block. The proposer infers params from the first occurrence's
/// free variables (the called fns: one, two, ..., six).
#[test]
fn propose_extract_finds_verbatim_dup() {
    let d = sandbox("verbatim");
    fs::create_dir_all(d.join("src")).unwrap();
    let rust = "fn alpha() {\n    let a = one();\n    let b = two();\n    let c = three();\n    let d = four();\n    let e = five();\n    let f = six();\n}\nfn beta() {\n    let a = one();\n    let b = two();\n    let c = three();\n    let d = four();\n    let e = five();\n    let f = six();\n}\n";
    fs::write(d.join("src/dup.rs"), rust).unwrap();
    let prog = concat!(
        "rel touch(p: file).\n",
        "touch(p) <- scan(\"WORK\", \"src/**/*.rs\", p, rev).\n",
        "? propose_extract(p, lo, hi, param).\n",
    );
    let (code, out, err) = run(&d, prog);
    assert_eq!(code, 0, "stderr: {err}");
    assert!(out.contains("src/dup.rs"), "path must appear in output:\n{out}");
    assert!(out.contains("one"), "free var 'one' must be a param:\n{out}");
}

/// (2) A file with no duplication: the relation is empty (no rows, no error).
#[test]
fn propose_extract_empty_on_unique_code() {
    let d = sandbox("unique");
    fs::create_dir_all(d.join("src")).unwrap();
    let rust = "fn alpha() {\n    let a = one();\n}\nfn beta() {\n    let b = two();\n}\nfn gamma() {\n    let c = three();\n}\n";
    fs::write(d.join("src/unique.rs"), rust).unwrap();
    let prog = concat!(
        "rel touch(p: file).\n",
        "touch(p) <- scan(\"WORK\", \"src/**/*.rs\", p, rev).\n",
        "? propose_extract(p, lo, hi, param).\n",
    );
    let (code, out, err) = run(&d, prog);
    assert_eq!(code, 0, "stderr: {err}");
    assert!(out.contains("(0 rows)"), "no proposals expected on unique code:\n{out}");
}

/// (3) `propose_extract` is a reserved name.
#[test]
fn propose_extract_is_reserved() {
    let d = sandbox("reserved");
    let (code, _out, err) = run(&d, "rel propose_extract(p: text).\n");
    assert_ne!(code, 0);
    assert!(err.contains("built-in"), "reserved-name error expected:\n{err}");
}

/// (4) `propose_clone(kernel, path, lo, hi, param)` runs all 9 kernels.
/// The ast-shape kernel finds renamed-variable duplication that verbatim misses.
#[test]
fn propose_clone_runs_multiple_kernels() {
    let d = sandbox("clone");
    fs::create_dir_all(d.join("src")).unwrap();
    let rust = "fn alpha() {\n    let result = compute(input);\n    map.insert(key, result);\n    return verify(result);\n}\nfn beta() {\n    let value = compute(input);\n    map.insert(key, value);\n    return verify(value);\n}\n";
    fs::write(d.join("src/dup.rs"), rust).unwrap();
    let prog = concat!(
        "rel touch(p: file).\n",
        "touch(p) <- scan(\"WORK\", \"src/**/*.rs\", p, rev).\n",
        "? propose_clone(kernel, p, lo, hi, param).\n",
    );
    let (code, out, err) = run(&d, prog);
    assert_eq!(code, 0, "stderr: {err}");
    assert!(out.contains("src/dup.rs"), "path must appear:\n{out}");
    assert!(
        out.contains("ast") || out.contains("tree") || out.contains("ddg"),
        "at least one structural kernel must find the renamed-var dup:\n{out}"
    );
}

/// (5) `propose_clone` is a reserved name.
#[test]
fn propose_clone_is_reserved() {
    let d = sandbox("clone_reserved");
    let (code, _out, err) = run(&d, "rel propose_clone(k: text, p: text).\n");
    assert_ne!(code, 0);
    assert!(err.contains("built-in"), "reserved-name error expected:\n{err}");
}
