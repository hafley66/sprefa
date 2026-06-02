//! The `where` query clause is gone; nesting subsumes every shape it covered.
//!
//!   - closure point query: pin a column with a literal head term (`? reaches("a", d).`)
//!   - plain Eq filter: literal head term (`? def("X", p).`)
//!   - regex/glob filter: a nested rule with the constraint in its body
//!   - seeding a closure FROM A RULE BODY now works (seeded reachability), the
//!     intentional new eval-semantics: `h(b) <- reaches(a, b), a = "X".`
//!   - an UNPINNED closure body-read still errors (no full Theta(V^2) materialize)
//!   - parsing `where` errors loudly with the migration recipe
//!
//! See plans/2026-06-02-kill-where-seed-closures-by-nesting.md.

use std::fs;
use std::path::{Path, PathBuf};
use std::process::Command;

const DL: &str = env!("CARGO_BIN_EXE_dl");

fn sandbox(tag: &str) -> PathBuf {
    let dir = std::env::temp_dir().join(format!("where_removed_{tag}"));
    let _ = fs::remove_dir_all(&dir);
    fs::create_dir_all(&dir).unwrap();
    dir
}

/// A tiny call graph: a -> b -> c, a -> d. As Rust so the same `ast` extraction
/// path the examples use produces the edges.
fn write_corpus(dir: &Path) {
    fs::create_dir_all(dir.join("src")).unwrap();
    fs::write(dir.join("src/lib.rs"), "\
fn a() { b(); d(); }
fn b() { c(); }
fn c() {}
fn d() {}
").unwrap();
}

/// Reaches over a free-function call graph (no qualified names, so node = ident).
const GRAPH: &str = r#"
rel fndef(name: text, path: file, s: int, e: int).
rel cs(callee: text, path: file, line: int).
rel calls(caller: text, callee: text).
rel reaches(src: text, dst: text).
fndef(name, path, s, e) <- scan("WORK","src/**/*.rs",path,rev),
  ast(path, rev, :rust, "(function_item name: (identifier) @name) @fn", s, e).
cs(callee, path, line) <- scan("WORK","src/**/*.rs",path,rev),
  ast(path, rev, :rust, "(call_expression function: (identifier) @callee)", line).
calls(caller, callee) <- fndef(caller, p, cs, ce), cs(callee, p, l), cs <= l, l <= ce, fndef(callee,_,_,_).
reaches(src, dst) <- closure(calls).
"#;

fn run(dir: &Path, prog: &str) -> (bool, String, String) {
    fs::write(dir.join("p.dl"), prog).unwrap();
    let out = Command::new(DL)
        .arg(dir.join("p.dl"))
        .args(["--root", dir.to_str().unwrap(), "--db", dir.join("db").to_str().unwrap()])
        .output().expect("run dl");
    (out.status.success(),
     String::from_utf8_lossy(&out.stdout).into_owned(),
     String::from_utf8_lossy(&out.stderr).into_owned())
}

/// Pull the dst column (last cell) of every data row out of a query block.
fn dsts(out: &str, after: &str) -> Vec<String> {
    let block = out.split(after).nth(1).unwrap_or("");
    block.lines()
        .take_while(|l| !l.contains("rows)"))
        .filter(|l| l.starts_with("  "))
        .map(|l| l.trim().rsplit('\t').next().unwrap().to_string())
        .collect()
}

#[test]
fn closure_seed_via_literal_head_matches() {
    let d = sandbox("seed_query");
    write_corpus(&d);
    let prog = format!("{GRAPH}\n? reaches(\"a\", dst).\n");
    let (ok, out, err) = run(&d, &prog);
    assert!(ok, "stderr: {err}");
    let mut got = dsts(&out, "? reaches");
    got.sort();
    // a reaches b, c, d (transitively); not a itself (acyclic).
    assert_eq!(got, vec!["b", "c", "d"], "out: {out}");
}

#[test]
fn closure_body_read_seedable_ok_and_matches_query() {
    let d = sandbox("seed_body");
    write_corpus(&d);
    let prog = format!(
        "{GRAPH}\nrel from_a(dst: text).\nfrom_a(b) <- reaches(a, b), a = \"a\".\n? from_a(dst).\n? reaches(\"a\", dst).\n");
    let (ok, out, err) = run(&d, &prog);
    assert!(ok, "stderr: {err}");
    let mut body = dsts(&out, "? from_a");
    let mut query = dsts(&out, "? reaches");
    body.sort(); query.sort();
    assert_eq!(body, vec!["b", "c", "d"], "body-seeded set: {out}");
    assert_eq!(body, query, "body-read must equal the query seed: {out}");
}

#[test]
fn closure_body_read_unpinned_errors() {
    let d = sandbox("unpinned");
    write_corpus(&d);
    let prog = format!("{GRAPH}\nrel all(a: text, b: text).\nall(a, b) <- reaches(a, b).\n? all(a, b).\n");
    let (ok, _out, err) = run(&d, &prog);
    assert!(!ok, "an unpinned closure body-read must error");
    assert!(err.contains("unpinned shape"), "stderr: {err}");
    assert!(err.contains("pinned to a literal"), "stderr: {err}");
}

#[test]
fn regex_filter_via_nested_rule() {
    let d = sandbox("regex");
    write_corpus(&d);
    // filter the fndef relation by a regex constraint in a nested rule body.
    let prog = format!(
        "{GRAPH}\nrel def(name: text).\ndef(n) <- fndef(n, _, _, _).\nrel just_a(name: text).\njust_a(n) <- def(n), n =~ \"^a$\".\n? just_a(name).\n");
    let (ok, out, err) = run(&d, &prog);
    assert!(ok, "stderr: {err}");
    assert!(out.contains("\n  a\n") || out.contains("  a\n  ("), "expected only 'a': {out}");
    assert!(!out.contains("  b\n") && !out.contains("  c\n"), "must exclude b,c: {out}");
}

#[test]
fn where_keyword_now_errors() {
    let d = sandbox("kw");
    write_corpus(&d);
    let prog = format!("{GRAPH}\n? reaches(src, dst) where src = \"a\".\n");
    let (ok, _out, err) = run(&d, &prog);
    assert!(!ok, "`where` must no longer parse");
    assert!(err.contains("`where` was removed"), "stderr: {err}");
}
