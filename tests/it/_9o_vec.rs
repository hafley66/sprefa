//! `(vec ...)`, `vec.iter`, `vec.len` and `vec.get` through the public compiler
//! (comptime rows) and `dl8 eval` (runtime closure) on every engine.

use super::_9e_generic_rules::{render_row, ENGINES};
use serde_json::{json, Value};
use std::collections::BTreeSet;
use std::path::Path;
use std::process::Command;

const RELATIONS: [&str; 8] = [
    "each",
    "size",
    "at_one",
    "built_each",
    "built_size",
    "same",
    "inner_size",
    "files",
];

const EXPECTED: [&str; 28] = [
    "at_one: const(\"n\"), ref(application(vec, [const(3)]))",
    "at_one: const(\"r\"), const(\"b\")",
    "built_each: const(\"e\"), const(0), const(\"e\")",
    "built_each: const(\"e\"), const(1), const(\"x\")",
    "built_each: const(\"e\"), const(2), const(\"e\")",
    "built_each: const(\"n\"), const(0), const(\"n\")",
    "built_each: const(\"n\"), const(1), const(\"x\")",
    "built_each: const(\"n\"), const(2), const(\"n\")",
    "built_each: const(\"r\"), const(0), const(\"r\")",
    "built_each: const(\"r\"), const(1), const(\"x\")",
    "built_each: const(\"r\"), const(2), const(\"r\")",
    "built_size: const(\"e\"), const(3)",
    "built_size: const(\"n\"), const(3)",
    "built_size: const(\"r\"), const(3)",
    "each: const(\"n\"), const(0), ref(application(vec, [const(1), const(2)]))",
    "each: const(\"n\"), const(1), ref(application(vec, [const(3)]))",
    "each: const(\"r\"), const(0), const(\"a\")",
    "each: const(\"r\"), const(1), const(\"b\")",
    "each: const(\"r\"), const(2), const(\"c\")",
    "files: const(\"e\"), ref(application(vec, []))",
    "files: const(\"n\"), ref(application(vec, [ref(application(vec, [const(1), const(2)])), ref(application(vec, [const(3)]))]))",
    "files: const(\"r\"), ref(application(vec, [const(\"a\"), const(\"b\"), const(\"c\")]))",
    "inner_size: const(0), const(2)",
    "inner_size: const(1), const(1)",
    "same: const(\"r\")",
    "size: const(\"e\"), const(0)",
    "size: const(\"n\"), const(2)",
    "size: const(\"r\"), const(3)",
];

fn dl8(args: &[&str], engine: &str) -> Value {
    let output = Command::new(env!("CARGO_BIN_EXE_dl8"))
        .args(args)
        .env("DL8_ENGINE", engine)
        .output()
        .unwrap();
    assert!(output.status.success(), "{engine}: {}", String::from_utf8_lossy(&output.stderr));
    serde_json::from_slice(&output.stdout).unwrap()
}

fn select(rows: &Value, names: &Value) -> Vec<String> {
    let rendered: BTreeSet<String> = rows
        .as_array()
        .unwrap()
        .iter()
        .map(|row| render_row(row, names))
        .filter(|row| RELATIONS.iter().any(|relation| row.starts_with(&format!("{relation}: "))))
        .collect();
    rendered.into_iter().collect()
}

#[test]
fn vec_items_len_and_index_agree_at_comptime_and_runtime_on_every_engine() {
    let root = Path::new(env!("CARGO_MANIFEST_DIR"));
    let source = root.join("fixtures/vec/0_items.dl7");
    let expected: Vec<&str> = EXPECTED.to_vec();
    let directory = std::env::temp_dir().join(format!("dl8-vec-{}", std::process::id()));
    std::fs::create_dir_all(&directory).unwrap();
    for engine in ENGINES {
        let program = dl8(&["compile", source.to_str().unwrap()], engine);
        assert_eq!(program["diagnostics"], json!([]), "{engine}");
        let names = &program["program"]["names"];
        assert_eq!(select(&program["compiler_rows"], names), expected, "{engine} comptime");
        let path = directory.join(format!("{}.json", engine.replace(':', "_")));
        std::fs::write(&path, serde_json::to_vec(&program).unwrap()).unwrap();
        let evaluated = dl8(&["eval", path.to_str().unwrap()], engine);
        assert_eq!(evaluated["diagnostics"], json!([]), "{engine}");
        assert_eq!(select(&evaluated["closure"], names), expected, "{engine} runtime");
    }
}

/// `[` no longer reads; it reports where it stands.
#[test]
fn a_bracket_is_a_reader_diagnostic() {
    let directory = std::env::temp_dir().join(format!("dl8-vec-bracket-{}", std::process::id()));
    std::fs::create_dir_all(&directory).unwrap();
    let path = directory.join("bracket.dl7");
    std::fs::write(&path, "(files: (* (list: any)))\n(files [1 2])\n").unwrap();
    let output = Command::new(env!("CARGO_BIN_EXE_dl8"))
        .arg("compile")
        .arg(&path)
        .output()
        .unwrap();
    let program: Value = serde_json::from_slice(&output.stdout).unwrap();
    let reasons: Vec<&Value> = program["diagnostics"]
        .as_array()
        .unwrap()
        .iter()
        .map(|diagnostic| &diagnostic["args"][3])
        .collect();
    assert_eq!(reasons, [&json!({"a": "unexpected_opening_bracket"})]);
}
