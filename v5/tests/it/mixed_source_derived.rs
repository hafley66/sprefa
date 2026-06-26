//! Regression: a relation written by BOTH a source rule (scan/match/...) and a
//! derived rule used to silently drop the scanned rows (reconcile fills them,
//! then `rebuild_derived` does a full `DELETE FROM rel` + recompute). The engine
//! now bails loudly and tells you to split the two rules into separate rels.

use std::fs;
use std::path::{Path, PathBuf};
use std::process::Command;

const DL: &str = env!("CARGO_BIN_EXE_dl");

fn sandbox(tag: &str) -> PathBuf {
    let dir = std::env::temp_dir().join(format!("mixed_source_derived_{tag}"));
    let _ = fs::remove_dir_all(&dir);
    fs::create_dir_all(&dir).unwrap();
    dir
}

fn run(dir: &Path, prog: &str) -> (i32, String) {
    fs::write(dir.join("p.dl"), prog).unwrap();
    let out = Command::new(DL)
        .arg(dir.join("p.dl"))
        .args(["--root", dir.to_str().unwrap(), "--db", dir.join("db").to_str().unwrap()])
        .output()
        .expect("run dl");
    (out.status.code().unwrap_or(-1), String::from_utf8_lossy(&out.stderr).into_owned())
}

#[test]
fn mixed_source_and_derived_rule_for_one_rel_bails() {
    let d = sandbox("bail");
    fs::create_dir_all(d.join("src")).unwrap();
    fs::write(d.join("src/a.txt"), "hello\n").unwrap();

    // `mixed` is fed by a scan rule (source) AND a plain derived rule.
    let prog = r#"
rel seen(p: file).
seen(p) <- scan("WORK", "src/**/*.txt", p, rev), match(p, rev, /./, line).
rel mixed(x: text).
mixed(p) <- scan("WORK", "src/**/*.txt", p, rev), match(p, rev, /./, line).
mixed(x) <- seen(x).
? mixed(x).
"#;
    let (code, stderr) = run(&d, prog);
    assert_ne!(code, 0, "expected a non-zero exit; stderr: {stderr}");
    assert!(
        stderr.contains("both a source rule") && stderr.contains("mixed"),
        "expected the mixed-rel bail naming 'mixed'; got: {stderr}"
    );
}

#[test]
fn split_source_and_derived_into_two_rels_is_fine() {
    let d = sandbox("split");
    fs::create_dir_all(d.join("src")).unwrap();
    fs::write(d.join("src/a.txt"), "hello\n").unwrap();

    // The sanctioned shape: scan into one rel, derive into another, union in a
    // third derived rule. No rel mixes the two rule kinds.
    let prog = r#"
rel spin(p: file).
spin(p) <- scan("WORK", "src/**/*.txt", p, rev), match(p, rev, /./, line).
rel dpin(p: file).
dpin(p) <- spin(p).
rel both(p: file).
both(p) <- spin(p).
both(p) <- dpin(p).
? both(p).
"#;
    let (code, stderr) = run(&d, prog);
    assert_eq!(code, 0, "split program should succeed; stderr: {stderr}");
}
