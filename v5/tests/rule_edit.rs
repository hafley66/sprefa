//! Editing a source rule must invalidate cached extractions for files whose
//! content did not change: rows are a function of (file content, rule), but
//! the reconcile fast path keys on file hash alone. Found by dogfooding: a
//! tightened banned-word regex in .dl/rails.dl silently did not enforce until
//! the target files happened to change (`source_rule_digests`).

use std::fs;
use std::path::{Path, PathBuf};
use std::process::Command;

const DL: &str = env!("CARGO_BIN_EXE_dl");

fn sandbox(tag: &str) -> PathBuf {
    let dir = std::env::temp_dir().join(format!("rule_edit_{tag}"));
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

fn prog(re: &str) -> String {
    format!(concat!(
        "rel hit(p: file, l: int).\n",
        "hit(p, l) <- scan(\"WORK\", \"src/**/*.rs\", p, rev), match(p, rev, /{}/, l).\n",
        "? hit(p, l).\n"), re)
}

/// Same db, same untouched file, three program versions: the rows must follow
/// the rule text in both directions (match -> no-match -> match again).
#[test]
fn edited_regex_reextracts_unchanged_files() {
    let d = sandbox("regex");
    fs::create_dir_all(d.join("src")).unwrap();
    fs::write(d.join("src/x.rs"), "fn alpha() {}\n").unwrap();

    let (code, out, err) = run(&d, &prog("alpha"));
    assert_eq!(code, 0, "{err}");
    assert!(out.contains("src/x.rs"), "v1 must match:\n{out}");

    // file untouched, rule no longer matches: stale rows must not survive
    let (code, out, err) = run(&d, &prog("zzznever"));
    assert_eq!(code, 0, "{err}");
    assert!(!out.contains("src/x.rs"), "v2 stale rows survived the rule edit:\n{out}");

    // and back: re-extraction must produce rows again, not just delete
    let (code, out, err) = run(&d, &prog("alpha"));
    assert_eq!(code, 0, "{err}");
    assert!(out.contains("src/x.rs"), "v3 must match again:\n{out}");
}

/// An unedited program on a warm db stays on the fast path: the digest check
/// alone must not force re-extraction (stderr reports 0 parsed on the second
/// identical run if the engine prints reconcile stats; assert via row output
/// stability instead, which is the observable contract).
#[test]
fn unedited_program_keeps_rows_on_warm_db() {
    let d = sandbox("warm");
    fs::create_dir_all(d.join("src")).unwrap();
    fs::write(d.join("src/x.rs"), "fn alpha() {}\n").unwrap();
    let p = prog("alpha");
    let (_, out1, _) = run(&d, &p);
    let (_, out2, _) = run(&d, &p);
    assert_eq!(out1, out2, "identical program + warm db must be stable");
    assert!(out2.contains("src/x.rs"));
}

/// The derived twin: editing a derived rule's filter on a warm db (no file or
/// source-rule change) must rebuild the derived layer (`derived:program`
/// digest), in both directions.
#[test]
fn edited_derived_rule_rebuilds_on_warm_db() {
    let d = sandbox("derived");
    fs::create_dir_all(d.join("src")).unwrap();
    fs::write(d.join("src/x.rs"), "fn alpha() {}\nfn beta() {}\n").unwrap();
    let prog = |needle: &str| format!(concat!(
        "rel hit(p: file, l: int).\n",
        "hit(p, l) <- scan(\"WORK\", \"src/**/*.rs\", p, rev), match(p, rev, /fn (?<w>\\w+)/, l).\n",
        "rel keep(p: file, l: int).\n",
        "keep(p, l) <- hit(p, l), l {}.\n",
        "? keep(p, l).\n"), needle);

    let (code, out, err) = run(&d, &prog("= 1"));
    assert_eq!(code, 0, "{err}");
    assert!(out.contains("1") && !out.contains("2"), "v1 keeps line 1 only:\n{out}");

    let (code, out, err) = run(&d, &prog("= 2"));
    assert_eq!(code, 0, "{err}");
    assert!(out.contains("2"), "v2 must rebuild to line 2:\n{out}");
    assert!(!out.contains("\t1\n"), "stale derived row survived the edit:\n{out}");
}
