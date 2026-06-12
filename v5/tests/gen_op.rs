//! The `gen` codegen sink. File form: `gen("path", "tmpl {x}") <- body.` renders
//! body rows (deterministic order) into a file, grouping by rendered path.
//! Splice form: `gen(p, l0, l1, "tmpl {x}") <- body.` replaces the lines strictly
//! between two marker lines (the `comment` op's paired coordinates). Both skip
//! the write when bytes already match, so a converged tick is a no-op.

use std::fs;
use std::path::{Path, PathBuf};
use std::process::Command;

const DL: &str = env!("CARGO_BIN_EXE_dl");

fn sandbox(tag: &str) -> PathBuf {
    let dir = std::env::temp_dir().join(format!("gen_op_{tag}"));
    let _ = fs::remove_dir_all(&dir);
    fs::create_dir_all(&dir).unwrap();
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

#[test]
fn file_form_renders_rows_in_deterministic_order() {
    let d = sandbox("file");
    let (code, _, err) = run(&d, concat!(
        "rel edge(a: text, n: int).\n",
        "edge(\"beta\", 1).\n",
        "edge(\"alpha\", 3).\n",
        "gen(\"out/report.md\", \"| {a} | {n} |\") <- edge(a, n).\n"));
    assert_eq!(code, 0, "{err}");
    let got = fs::read_to_string(d.join("out/report.md")).unwrap();
    assert_eq!(got, "| alpha | 3 |\n| beta | 1 |\n");
}

#[test]
fn path_template_groups_rows_per_file() {
    let d = sandbox("group");
    let (code, _, err) = run(&d, concat!(
        "rel kv(f: text, v: text).\n",
        "kv(\"x\", \"1\").\n",
        "kv(\"y\", \"2\").\n",
        "gen(\"out/{f}.txt\", \"{v}\") <- kv(f, v).\n"));
    assert_eq!(code, 0, "{err}");
    assert_eq!(fs::read_to_string(d.join("out/x.txt")).unwrap(), "1\n");
    assert_eq!(fs::read_to_string(d.join("out/y.txt")).unwrap(), "2\n");
}

/// A converged gen must not touch the file. Proof without sleeping: make the
/// generated file read-only; the second run succeeds only if it skips the write.
#[test]
fn matching_bytes_skip_the_write() {
    let d = sandbox("idem");
    let prog = concat!(
        "rel edge(a: text).\n",
        "edge(\"only\").\n",
        "gen(\"out/r.txt\", \"{a}\") <- edge(a).\n");
    let (code, _, err) = run(&d, prog);
    assert_eq!(code, 0, "{err}");
    let target = d.join("out/r.txt");
    let mut perms = fs::metadata(&target).unwrap().permissions();
    perms.set_readonly(true);
    fs::set_permissions(&target, perms).unwrap();
    let (code, _, err) = run(&d, prog);
    assert_eq!(code, 0, "second run must skip the write entirely: {err}");
}

#[test]
fn escaping_path_is_rejected() {
    let d = sandbox("escape");
    fs::create_dir_all(d.join("inner")).unwrap();
    let prog = concat!(
        "rel edge(a: text).\n",
        "edge(\"x\").\n",
        "gen(\"../oops.txt\", \"{a}\") <- edge(a).\n");
    fs::write(d.join("inner/p.dl"), prog).unwrap();
    let out = Command::new(DL)
        .arg(d.join("inner/p.dl"))
        .args(["--root", d.join("inner").to_str().unwrap(),
               "--db", d.join("db").to_str().unwrap()])
        .output().expect("run dl");
    assert_eq!(out.status.code().unwrap_or(-1), 1, "path escape must be a loud error");
    assert!(!d.join("oops.txt").exists());
}

/// Two gen rules splicing two regions of ONE file must batch into a single
/// write: the line coordinates come from the pre-tick content, so a second
/// rule re-reading a file the first rule already grew would land stale.
#[test]
fn two_gen_rules_splice_one_file_in_one_write() {
    let d = sandbox("two_rules");
    fs::write(d.join("doc.md"),
        "<!-- BEGIN: a -->\nstale\n<!-- END: -->\n<!-- BEGIN: b -->\nstale\n<!-- END: -->\n").unwrap();
    let prog = concat!(
        "rel block(p: file, l0: int, l1: int, name: text).\n",
        "block(p, l0, l1, name) <- scan(\"WORK\", \"*.md\", p, rev), ",
        "comment(p, rev, /BEGIN: $name -->/, /END:/, l0, l1, name).\n",
        "rel xs(x: text).\n",
        "xs(\"x1\").\nxs(\"x2\").\n",
        "rel ys(y: text).\n",
        "ys(\"y1\").\n",
        "gen(p, l0, l1, \"- {x}\") <- block(p, l0, l1, \"a\"), xs(x).\n",
        "gen(p, l0, l1, \"* {y}\") <- block(p, l0, l1, \"b\"), ys(y).\n");
    let (code, _, err) = run(&d, prog);
    assert_eq!(code, 0, "{err}");
    let got = fs::read_to_string(d.join("doc.md")).unwrap();
    assert_eq!(got,
        "<!-- BEGIN: a -->\n- x1\n- x2\n<!-- END: -->\n<!-- BEGIN: b -->\n* y1\n<!-- END: -->\n");

    let target = d.join("doc.md");
    let mut perms = fs::metadata(&target).unwrap().permissions();
    perms.set_readonly(true);
    fs::set_permissions(&target, perms).unwrap();
    let (code, _, err) = run(&d, prog);
    assert_eq!(code, 0, "warm two-rule splice must converge without writing: {err}");
}

/// The marker-splice loop: comment regions give the coordinates, gen rewrites
/// the lines between the markers, the markers and surrounding text survive,
/// and a second warm run converges (no write).
#[test]
fn splice_form_rewrites_between_markers_and_converges() {
    let d = sandbox("splice");
    fs::write(d.join("README.md"),
        "# Title\n<!-- BEGIN: items -->\nstale line\n<!-- END: -->\ntail\n").unwrap();
    let prog = concat!(
        "rel block(p: file, l0: int, l1: int, name: text).\n",
        "block(p, l0, l1, name) <- scan(\"WORK\", \"*.md\", p, rev), ",
        "comment(p, rev, /BEGIN: $name -->/, /END:/, l0, l1, name).\n",
        "rel item(x: text).\n",
        "item(\"alpha\").\n",
        "item(\"beta\").\n",
        "gen(p, l0, l1, \"- {x}\") <- block(p, l0, l1, \"items\"), item(x).\n");
    let (code, _, err) = run(&d, prog);
    assert_eq!(code, 0, "{err}");
    let got = fs::read_to_string(d.join("README.md")).unwrap();
    assert_eq!(got,
        "# Title\n<!-- BEGIN: items -->\n- alpha\n- beta\n<!-- END: -->\ntail\n");

    // Converged: the regenerated content matches, so the file is untouched.
    let target = d.join("README.md");
    let mut perms = fs::metadata(&target).unwrap().permissions();
    perms.set_readonly(true);
    fs::set_permissions(&target, perms).unwrap();
    let (code, _, err) = run(&d, prog);
    assert_eq!(code, 0, "warm splice must converge without writing: {err}");
}
