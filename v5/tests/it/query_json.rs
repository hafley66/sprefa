//! `--query-json` emits one JSON object per `?` query (JSON-lines):
//! {query, columns, rows, count}. Text cells stay JSON strings, int cells stay
//! JSON numbers. Covers both the SQL-view path and the seeded-closure path.

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
        .args(["--root", dir.to_str().unwrap(), "--db", dir.join("db").to_str().unwrap(), "--query-json"])
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
