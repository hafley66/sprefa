//! String kernel ops through the public compiler on every evaluation engine.

use serde_json::{json, Value};
use std::path::Path;
use std::process::Command;

const ENGINES: [&str; 2] = ["ir:sqlite", "ir:dd"];

fn cell(arg: &Value) -> Value {
    let payload = &arg["args"][0];
    payload.get("s").unwrap_or(payload).clone()
}

fn rows(closure: &Value, relation: &Value) -> Value {
    let mut rows = closure["closure"]
        .as_array()
        .unwrap()
        .iter()
        .filter(|row| &row["rel"] == relation)
        .map(|row| {
            Value::Array(
                row["args"]
                    .as_array()
                    .unwrap()
                    .iter()
                    .map(cell)
                    .collect(),
            )
        })
        .collect::<Vec<_>>();
    rows.sort_by_key(|row| row.to_string());
    Value::Array(rows)
}

/// Compiles `fixtures/str/<stem>.dl7`, evaluates it on every engine, and
/// compares the named relations' rows exactly.
fn run(stem: &str, expected: Value) {
    let root = Path::new(env!("CARGO_MANIFEST_DIR"));
    let compiled = Command::new(env!("CARGO_BIN_EXE_dl8"))
        .current_dir(root)
        .arg("compile")
        .arg(root.join(format!("fixtures/str/{stem}.dl7")))
        .output()
        .unwrap();
    assert!(
        compiled.status.success(),
        "{}",
        String::from_utf8_lossy(&compiled.stderr)
    );
    let program: Value = serde_json::from_slice(&compiled.stdout).unwrap();
    assert_eq!(program["diagnostics"], json!([]));
    let directory = std::env::temp_dir().join(format!("dl8-str-{stem}-{}", std::process::id()));
    std::fs::create_dir_all(&directory).unwrap();
    let program_path = directory.join("program.json");
    std::fs::write(&program_path, &compiled.stdout).unwrap();
    for engine in ENGINES {
        let evaluated = Command::new(env!("CARGO_BIN_EXE_dl8"))
            .arg("eval")
            .arg(&program_path)
            .env("DL8_ENGINE", engine)
            .output()
            .unwrap();
        assert!(
            evaluated.status.success(),
            "{engine}: {}",
            String::from_utf8_lossy(&evaluated.stderr)
        );
        let closure: Value = serde_json::from_slice(&evaluated.stdout).unwrap();
        assert_eq!(closure["diagnostics"], json!([]), "{engine}");
        let names = &program["program"]["names"];
        let actual: serde_json::Map<String, Value> = expected
            .as_object()
            .unwrap()
            .keys()
            .map(|name| (name.clone(), rows(&closure, &names[name])))
            .collect();
        assert_eq!(Value::Object(actual), expected, "{engine}");
    }
    std::fs::remove_dir_all(directory).unwrap();
}

#[test]
fn v5_ports() {
    run(
        "0_v5_ports",
        json!({
            "length": [["  User  ", 8], ["", 0], ["UserId", 6], ["héllo Ünï", 9], ["userId", 6]],
            "lowered": [["  User  ", "  user  "], ["", ""], ["UserId", "userid"], ["héllo Ünï", "héllo ünï"], ["userId", "userid"]],
            "uppered": [["  User  ", "  USER  "], ["", ""], ["UserId", "USERID"], ["héllo Ünï", "HÉLLO ÜNÏ"], ["userId", "USERID"]],
            "lcfirsted": [["  User  ", "  User  "], ["", ""], ["UserId", "userId"], ["héllo Ünï", "héllo Ünï"], ["userId", "userId"]],
            "ucfirsted": [["  User  ", "  User  "], ["", ""], ["UserId", "UserId"], ["héllo Ünï", "Héllo Ünï"], ["userId", "UserId"]],
            "trimmed": [["  User  ", "User"], ["", ""], ["UserId", "UserId"], ["héllo Ünï", "héllo Ünï"], ["userId", "userId"]],
            "prefix_stripped": [["  User  ", "  User  "], ["", ""], ["UserId", "UserId"], ["héllo Ünï", "héllo Ünï"], ["userId", "Id"]],
            "suffix_stripped": [["  User  ", "  User  "], ["", ""], ["UserId", "User"], ["héllo Ünï", "héllo Ünï"], ["userId", "user"]],
            "replaced": [["  User  ", "  Us3r  "], ["", ""], ["UserId", "Us3rId"], ["héllo Ünï", "héllo Ünï"], ["userId", "us3rId"]],
            "split_part": [["a.b.c", ".", -1, "c"], ["a.b.c", ".", 0, "a"], ["abc", ",", 0, "abc"]],
        }),
    );
}

#[test]
fn conversions() {
    run(
        "1_conversions",
        json!({
            "number_text": [[-7, "-7"], [0, "0"], [3, "3"]],
            "numeral_value": [["+4", 4], ["-5", -5], ["12", 12]],
        }),
    );
}

#[test]
fn filters() {
    run(
        "2_filters",
        json!({
            "under_src": [["src/lib/mod.rs"], ["src/main.rs"]],
            "has_lib": [["src/lib/mod.rs"]],
            "has_umlaut": [["héllo/wörld"]],
            "empty_prefix": [[""], ["docs/readme.md"], ["héllo/wörld"], ["src/lib/mod.rs"], ["src/main.rs"]],
            "never": [],
            "outside_src": [[""], ["docs/readme.md"], ["héllo/wörld"]],
            "without_lib": [[""], ["docs/readme.md"], ["héllo/wörld"], ["src/main.rs"]],
        }),
    );
}

#[test]
fn case() {
    run(
        "3_case",
        json!({
            "snaked": [["", ""], ["HTTPServer", "http_server"], ["UserId", "user_id"], ["héllo wörld", "héllo_wörld"], ["user-id", "user_id"], ["user_id", "user_id"]],
            "cameled": [["", ""], ["HTTPServer", "httpServer"], ["UserId", "userId"], ["héllo wörld", "hélloWörld"], ["user-id", "userId"], ["user_id", "userId"]],
            "pascaled": [["", ""], ["HTTPServer", "HttpServer"], ["UserId", "UserId"], ["héllo wörld", "HélloWörld"], ["user-id", "UserId"], ["user_id", "UserId"]],
            "kebabed": [["", ""], ["HTTPServer", "http-server"], ["UserId", "user-id"], ["héllo wörld", "héllo-wörld"], ["user-id", "user-id"], ["user_id", "user-id"]],
        }),
    );
}

#[test]
fn escaping() {
    run(
        "4_escaping",
        json!({
            "escaped": [["", ""], ["back\\slash", "back\\\\slash"], ["héllo", "héllo"], ["it's", "it\\'s"], ["line\nbreak", "line\\nbreak"], ["plain", "plain"], ["say \"hi\"", "say \\\"hi\\\""]],
            "jsoned": [["", "\"\""], ["back\\slash", "\"back\\\\slash\""], ["héllo", "\"héllo\""], ["it's", "\"it's\""], ["line\nbreak", "\"line\\nbreak\""], ["plain", "\"plain\""], ["say \"hi\"", "\"say \\\"hi\\\"\""]],
        }),
    );
}
