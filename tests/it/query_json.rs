//! `--query-json` emits one JSON object per `?` query (JSON-lines):
//! {query, columns, rows, count}. Text cells stay JSON strings, int cells stay
//! JSON numbers. Covers both the SQL-view path and the seeded-closure path.
//!
//! `--format json` (added alongside `--query-json`, same query CLI path) emits
//! a plain JSON array of row-objects instead, each row keyed by column name —
//! no `{query, columns, ...}` envelope. Same two paths (SQL view + seeded
//! closure), see `format_json_view_and_seeded_closure` below.

use std::fs;
use std::path::PathBuf;
use std::process::Command;

const DL: &str = env!("CARGO_BIN_EXE_dl");

fn sandbox(tag: &str) -> PathBuf {
    let dir = std::env::temp_dir().join(format!("query_json_{tag}"));
    let _ = fs::remove_dir_all(&dir);
    fs::create_dir_all(&dir).unwrap();
    dir
}

fn run_json(dir: &PathBuf, prog: &str) -> Vec<serde_json::Value> {
    fs::write(dir.join("p.dl"), prog).unwrap();
    let out = Command::new(DL)
        .arg(dir.join("p.dl"))
        .args(["--db", dir.join("db").to_str().unwrap(), "--query-json"])
        .current_dir(dir)
        .output().expect("run dl");
    assert!(out.status.success(), "stderr: {}", String::from_utf8_lossy(&out.stderr));
    String::from_utf8_lossy(&out.stdout)
        .lines()
        .filter(|l| !l.trim().is_empty())
        .map(|l| serde_json::from_str(l).unwrap_or_else(|e| panic!("bad json line `{l}`: {e}")))
        .collect()
}

#[test]
fn query_json_view_and_seeded_closure() {
    let d = sandbox("shape");
    fs::create_dir_all(d.join("src")).unwrap();
    fs::write(d.join("src/a.rs"), "fn alpha() {}\nfn beta() {}\n").unwrap();

    // A plain view query (def: text + int columns) and a seeded closure query.
    let prog = r#"
rel fn_def(name: text, path: file, line: int).
rel edge(a: text, b: text).
rel reaches(a: text, b: text).
fn_def(name, path, line) <- scan("WORK","src/**/*.rs",path,rev),
  ast(path, rev, :rust, "(function_item name: (identifier) @name) @fn", line).
edge("alpha","beta") <- scan("WORK","src/a.rs",p,rev), match(p,rev,/./,line).
reaches(a,b) <- closure(edge).
? fn_def(name, path, line).
? reaches("alpha", dst).
"#;
    let recs = run_json(&d, prog);
    assert_eq!(recs.len(), 2, "two queries -> two json lines: {recs:?}");

    let def = &recs[0];
    assert_eq!(def["query"], "fn_def");
    assert_eq!(def["columns"], serde_json::json!(["name", "path", "line"]));
    assert_eq!(def["count"], 2);
    // int column stays a JSON number, not a string.
    assert!(def["rows"][0][2].is_number(), "line col must be a number: {def}");
    assert!(def["rows"][0][0].is_string(), "name col must be a string: {def}");

    let reaches = &recs[1];
    assert_eq!(reaches["query"], "reaches");
    assert_eq!(reaches["count"], 1);
    assert_eq!(reaches["rows"], serde_json::json!([["alpha", "beta"]]));
}

/// Run a `--format json` program, parsing each stdout line as one JSON array
/// (one array per `?` query, same per-query granularity as `--query-json` —
/// see the module doc for why there is no per-row streaming variant).
fn run_format_json(dir: &PathBuf, prog: &str) -> Vec<serde_json::Value> {
    fs::write(dir.join("p.dl"), prog).unwrap();
    let out = Command::new(DL)
        .arg(dir.join("p.dl"))
        .args(["--db", dir.join("db").to_str().unwrap(), "--format", "json"])
        .current_dir(dir)
        .output().expect("run dl");
    assert!(out.status.success(), "stderr: {}", String::from_utf8_lossy(&out.stderr));
    String::from_utf8_lossy(&out.stdout)
        .lines()
        .filter(|l| l.trim_start().starts_with('['))
        .map(|l| serde_json::from_str(l).unwrap_or_else(|e| panic!("bad json line `{l}`: {e}")))
        .collect()
}

#[test]
fn format_json_view_and_seeded_closure() {
    let d = sandbox("format_shape");
    fs::create_dir_all(d.join("src")).unwrap();
    fs::write(d.join("src/a.rs"), "fn alpha() {}\nfn beta() {}\n").unwrap();

    let prog = r#"
rel fn_def(name: text, path: file, line: int).
rel edge(a: text, b: text).
rel reaches(a: text, b: text).
fn_def(name, path, line) <- scan("WORK","src/**/*.rs",path,rev),
  ast(path, rev, :rust, "(function_item name: (identifier) @name) @fn", line).
edge("alpha","beta") <- scan("WORK","src/a.rs",p,rev), match(p,rev,/./,line).
reaches(a,b) <- closure(edge).
? fn_def(name, path, line).
? reaches("alpha", dst).
"#;
    let arrays = run_format_json(&d, prog);
    assert_eq!(arrays.len(), 2, "two queries -> two json array lines: {arrays:?}");

    // No envelope: each element is a bare array; row objects are keyed by
    // column name directly, not nested under "rows".
    let defs = arrays[0].as_array().expect("fn_def line must be a JSON array");
    assert_eq!(defs.len(), 2);
    assert!(defs[0]["line"].is_number(), "line col must be a number: {defs:?}");
    assert!(defs[0]["name"].is_string(), "name col must be a string: {defs:?}");
    assert!(defs.iter().any(|row| row["name"] == "alpha"), "expected alpha row: {defs:?}");

    // Column keys mirror the query's own head terms: the pinned literal side
    // (`"alpha"`) keys off the declared column name (`a`), the free side
    // (`dst`) keys off the query's own variable name — same header rule the
    // `--query-json` NDJSON path uses.
    let reaches = arrays[1].as_array().expect("reaches line must be a JSON array");
    assert_eq!(reaches, &vec![serde_json::json!({"a": "alpha", "dst": "beta"})]);
}
