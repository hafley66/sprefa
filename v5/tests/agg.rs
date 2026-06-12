//! Aggregation (spec T4): head-position `count`/`sum`/`min`/`max` lower to a
//! `SELECT ... GROUP BY`; the result rel is ordinary (feeds other rules); an agg
//! or negation edge inside a recursive cycle is a `not-stratified` error. Plus the
//! bonus typecheck-hole fix: a literal whose type conflicts with its column is a
//! `brand-mismatch` instead of a SQLite datatype crash at tick time.

use std::fs;
use std::path::{Path, PathBuf};
use std::process::Command;

const DL: &str = env!("CARGO_BIN_EXE_dl");

fn sandbox(tag: &str) -> PathBuf {
    let dir = std::env::temp_dir().join(format!("agg_{tag}"));
    let _ = fs::remove_dir_all(&dir);
    fs::create_dir_all(&dir).unwrap();
    dir
}

/// Run `dl prog` with `--root` defaulting to the sandbox dir.
fn run(dir: &Path, prog: &str) -> (i32, String, String) {
    run_root(dir, prog, dir)
}

/// Run with an explicit `--root` (the gate points at the real sprefa checkout).
fn run_root(dir: &Path, prog: &str, root: &Path) -> (i32, String, String) {
    fs::write(dir.join("p.dl"), prog).unwrap();
    let out = Command::new(DL)
        .arg(dir.join("p.dl"))
        .args(["--root", root.to_str().unwrap(), "--db", dir.join("db").to_str().unwrap()])
        .output().expect("run dl");
    (out.status.code().unwrap_or(-1),
     String::from_utf8_lossy(&out.stdout).into_owned(),
     String::from_utf8_lossy(&out.stderr).into_owned())
}

/// Parse `? rel => ...` block rows into (col0, col1) pairs.
fn rows_of(out: &str, rel: &str) -> Vec<(String, String)> {
    let block = out.split(&format!("? {rel} =>")).nth(1).unwrap_or("");
    let mut rows = Vec::new();
    // The first line of the block is the header (`col0\tcol1`); skip it.
    for line in block.lines().skip(1) {
        let l = line.trim();
        if l.is_empty() || l.starts_with('(') || l.starts_with('?') { if l.starts_with('?') { break; } continue; }
        let mut it = l.split('\t');
        if let (Some(a), Some(b)) = (it.next(), it.next()) {
            rows.push((a.to_string(), b.to_string()));
        }
    }
    rows
}

/// (1) THE GATE. `fan_out(F, count(T)) <- type_edge(F, T, _)` over v5's own
/// self-hosted type graph, run with `--root` at the real sprefa checkout (parent
/// of v5/). The 2026-06-06 reference numbers were Tok=21, BodyItem=9, Engine=9.
/// T1-T3 work (the `Tok::Scheme` literal token + new engine fields) drifted the
/// live counts to Tok=23, BodyItem=9, Engine=10, so this asserts the FRESH live
/// values and the top-3 fan-out ordering (Tok highest, then Engine, then the
/// DescKind/BodyItem 9-tie) instead. See the agent report for the drift note.
#[test]
fn gate_fan_out_over_type_edge() {
    let d = sandbox("gate");
    let sprefa_root = Path::new(env!("CARGO_MANIFEST_DIR")).parent().unwrap();
    let prog = r#"
rel seen(path: file).
seen(path) <- scan("WORK", "v5/src/**/*.rs", path, rev), match(path, rev, /./, line).
rel fan_out(f: text, n: int).
fan_out(f, count(t)) <- type_edge(f, t, _).
? fan_out(f, n).
"#;
    let (code, out, err) = run_root(&d, prog, sprefa_root);
    assert_eq!(code, 0, "gate run failed:\nstdout={out}\nstderr={err}");
    let rows = rows_of(&out, "fan_out");
    let get = |name: &str| -> i64 {
        rows.iter().find(|(f, _)| f == name).map(|(_, n)| n.parse().unwrap())
            .unwrap_or_else(|| panic!("no fan_out row for {name}\nout={out}"))
    };
    // Live values (ProjectCx went 8 -> 11 with the Kotlin resolver: content
    // reader + kotlin index fields, tying BodyItem and displacing Engine; Tok
    // went 23 -> 28 with the five arithmetic operator tokens).
    assert_eq!(get("Tok"), 28, "Tok fan-out drifted again: {out}");
    assert_eq!(get("BodyItem"), 11, "BodyItem fan-out drifted: {out}");
    assert_eq!(get("ProjectCx"), 11, "ProjectCx fan-out drifted: {out}");
    assert_eq!(get("Engine"), 10, "Engine fan-out drifted again: {out}");
    // Ordering: Tok > {BodyItem, ProjectCx} > Engine > the rest.
    let mut sorted: Vec<(i64, String)> = rows.iter()
        .map(|(f, n)| (n.parse::<i64>().unwrap(), f.clone())).collect();
    sorted.sort_by(|a, b| b.0.cmp(&a.0));
    assert_eq!(sorted[0].1, "Tok", "top fan-out drifted: {:?}", &sorted[..4.min(sorted.len())]);
    let tied: Vec<&str> = sorted[1..3].iter().map(|(_, f)| f.as_str()).collect();
    assert!(tied.contains(&"BodyItem") && tied.contains(&"ProjectCx"),
        "tied pair drifted: {:?}", &sorted[..4.min(sorted.len())]);
    assert_eq!(sorted[3].1, "Engine", "rank-4 drifted: {:?}", &sorted[..5.min(sorted.len())]);
    assert!(sorted[0].0 > sorted[1].0 && sorted[1].0 == sorted[2].0 && sorted[2].0 > sorted[3].0,
        "Tok above the 11-11 tie, tie above Engine: {:?}", &sorted[..4.min(sorted.len())]);
}

/// (2) count/sum/min/max basics on a small fixture: three `fn` lines at 1,2,3
/// give count=3, min=1, max=3, sum=6.
#[test]
fn count_sum_min_max_basics() {
    let d = sandbox("basics");
    fs::create_dir_all(d.join("src")).unwrap();
    fs::write(d.join("src/x.rs"), "fn alpha() {}\nfn beta() {}\nfn gamma() {}\n").unwrap();
    let prog = r#"
rel fns(p: file, line: int).
fns(p, line) <- scan("WORK", "src/*.rs", p, rev), match(p, rev, /fn /, line).
rel cnt(p: file, v: int).
cnt(p, count(line)) <- fns(p, line).
rel lo(p: file, v: int).
lo(p, min(line)) <- fns(p, line).
rel hi(p: file, v: int).
hi(p, max(line)) <- fns(p, line).
rel tot(p: file, v: int).
tot(p, sum(line)) <- fns(p, line).
? cnt(p, v).
? lo(p, v).
? hi(p, v).
? tot(p, v).
"#;
    let (code, out, err) = run(&d, prog);
    assert_eq!(code, 0, "run failed:\nstdout={out}\nstderr={err}");
    assert_eq!(rows_of(&out, "cnt"), vec![("src/x.rs".into(), "3".into())], "count: {out}");
    assert_eq!(rows_of(&out, "lo"), vec![("src/x.rs".into(), "1".into())], "min: {out}");
    assert_eq!(rows_of(&out, "hi"), vec![("src/x.rs".into(), "3".into())], "max: {out}");
    assert_eq!(rows_of(&out, "tot"), vec![("src/x.rs".into(), "6".into())], "sum: {out}");
}

/// An aggregated relation is ordinary: it feeds a downstream derived rule. Only
/// files with >2 `fn`s survive the filter.
#[test]
fn agg_result_feeds_another_rule() {
    let d = sandbox("feeds");
    fs::create_dir_all(d.join("src")).unwrap();
    fs::write(d.join("src/x.rs"), "fn alpha() {}\nfn beta() {}\nfn gamma() {}\n").unwrap();
    fs::write(d.join("src/y.rs"), "fn solo() {}\n").unwrap();
    let prog = r#"
rel fns(p: file, line: int).
fns(p, line) <- scan("WORK", "src/*.rs", p, rev), match(p, rev, /fn /, line).
rel cnt(p: file, v: int).
cnt(p, count(line)) <- fns(p, line).
rel many(p: file).
many(p) <- cnt(p, c), c > 2.
? many(p).
"#;
    let (code, out, err) = run(&d, prog);
    assert_eq!(code, 0, "run failed:\nstdout={out}\nstderr={err}");
    assert!(out.contains("src/x.rs"), "x.rs (3 fns) should pass: {out}");
    assert!(!out.contains("src/y.rs"), "y.rs (1 fn) should be filtered out: {out}");
}

/// (3) An aggregation through its own SCC (`r` aggregates over a body that reads
/// `r`) is `not-stratified`.
#[test]
fn agg_through_own_scc_errors() {
    let d = sandbox("ns");
    fs::create_dir_all(d.join("src")).unwrap();
    fs::write(d.join("src/x.rs"), "fn alpha() {}\n").unwrap();
    let prog = r#"
rel edge(a: text, b: text).
edge(a, b) <- scan("WORK", "src/*.rs", a, rev), match(a, rev, /fn (\w+)/, b).
rel r(a: text, n: int).
r(a, count(b)) <- edge(a, b), r(b, _).
? r(a, n).
"#;
    let (code, _out, err) = run(&d, prog);
    assert_ne!(code, 0, "agg through own SCC must fail");
    assert!(err.contains("not-stratified"), "expected not-stratified: {err}");
}

/// (4) An agg combined with negation in a recursive cycle is `not-stratified`.
#[test]
fn agg_plus_negation_cycle_errors() {
    let d = sandbox("an");
    fs::create_dir_all(d.join("src")).unwrap();
    fs::write(d.join("src/x.rs"), "fn alpha() {}\n").unwrap();
    let prog = r#"
rel edge(x: text, y: text).
edge(x, y) <- scan("WORK", "src/*.rs", x, rev), match(x, rev, /fn (\w+)/, y).
rel a(x: text, n: int).
rel b(x: text).
a(x, count(y)) <- edge(x, y), !b(x).
b(x) <- a(x, _).
? a(x, n).
"#;
    let (code, _out, err) = run(&d, prog);
    assert_ne!(code, 0, "agg+negation cycle must fail");
    assert!(err.contains("not-stratified"), "expected not-stratified: {err}");
}

/// (5) BONUS FIX: a string literal in an int column is a `brand-mismatch` at
/// typecheck, not a SQLite datatype crash at tick time.
#[test]
fn string_literal_in_int_column_errors() {
    let d = sandbox("bonus_str");
    fs::create_dir_all(d.join("src")).unwrap();
    fs::write(d.join("src/x.rs"), "fn alpha() {}\n").unwrap();
    let prog = r#"
rel n(p: file, line: int).
n(p, "oops") <- scan("WORK", "src/*.rs", p, rev), match(p, rev, /fn /, line).
? n(p, line).
"#;
    let (code, _out, err) = run(&d, prog);
    assert_ne!(code, 0, "string in int column must fail");
    assert!(err.contains("brand-mismatch"), "expected brand-mismatch: {err}");
    assert!(err.contains("int column"), "diag should name the int column: {err}");
    assert!(!err.to_lowercase().contains("datatype mismatch"),
        "must not reach the SQLite datatype error: {err}");
}

/// An int literal in a path column is also a `brand-mismatch` (the symmetric hole).
#[test]
fn int_literal_in_path_column_errors() {
    let d = sandbox("bonus_int");
    fs::create_dir_all(d.join("src")).unwrap();
    fs::write(d.join("src/x.rs"), "fn alpha() {}\n").unwrap();
    let prog = r#"
rel m(p: file, line: int).
m(5, line) <- scan("WORK", "src/*.rs", p, rev), match(p, rev, /fn /, line).
? m(p, line).
"#;
    let (code, _out, err) = run(&d, prog);
    assert_ne!(code, 0, "int in path column must fail");
    assert!(err.contains("brand-mismatch"), "expected brand-mismatch: {err}");
}

/// An aggregate call in body position is a parse error (head-only).
#[test]
fn agg_in_body_is_parse_error() {
    let d = sandbox("body_agg");
    let prog = r#"
rel e(a: text, b: text).
rel r(a: text).
r(a) <- e(a, b), count(b).
? r(a).
"#;
    let (code, _out, err) = run(&d, prog);
    assert_ne!(code, 0, "agg in body must fail to parse");
    assert!(err.contains("only allowed in a rule head"), "expected head-only parse error: {err}");
}
