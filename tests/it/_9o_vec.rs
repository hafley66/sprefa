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

const INT_RELATIONS: [&str; 18] = [
    "each", "size", "next_index", "small", "back", "loose", "counted", "summed", "lowest", "counter",
    "pair", "diag", "three", "lens", "texts", "anyint", "fixed", "files",
];

/// Columns typed `int` hold the bare integer at comptime (`ir_lower::raw_columns`); `dl8 eval`
/// holds every column as a term id. Both print the same rows.
#[test]
fn int_columns_hold_bare_integers_and_print_as_at_runtime() {
    let root = Path::new(env!("CARGO_MANIFEST_DIR"));
    let source = root.join("fixtures/vec/1_int_columns.dl7");
    let directory = std::env::temp_dir().join(format!("dl8-int-columns-{}", std::process::id()));
    let dump = directory.join("ir");
    std::fs::create_dir_all(&dump).unwrap();
    let output = Command::new(env!("CARGO_BIN_EXE_dl8"))
        .args(["compile", source.to_str().unwrap()])
        .env("DL8_IR_DUMP", &dump)
        .output()
        .unwrap();
    assert!(output.status.success(), "{}", String::from_utf8_lossy(&output.stderr));
    let program: Value = serde_json::from_slice(&output.stdout).unwrap();
    assert_eq!(program["diagnostics"], json!([]));
    let names = &program["program"]["names"];
    let pick = |rows: &Value| -> Vec<String> {
        let rendered: BTreeSet<String> = rows
            .as_array()
            .unwrap()
            .iter()
            .map(|row| render_row(row, names))
            .filter(|row| INT_RELATIONS.iter().any(|relation| row.starts_with(&format!("{relation}: "))))
            .collect();
        rendered.into_iter().collect()
    };
    let comptime = pick(&program["compiler_rows"]);
    let raw_relations: usize = std::fs::read_dir(&dump)
        .unwrap()
        .map(|entry| {
            let ir: Value = serde_json::from_slice(&std::fs::read(entry.unwrap().path()).unwrap()).unwrap();
            ir["rels"]
                .as_array()
                .unwrap()
                .iter()
                .filter(|rel| rel["name"].as_str().unwrap().starts_with("__ir_n") && rel["cols"].as_array().unwrap().contains(&json!("Int")))
                .count()
        })
        .sum();
    assert!(raw_relations > 0, "no relation holds a bare integer column");
    let path = directory.join("program.json");
    std::fs::write(&path, serde_json::to_vec(&program).unwrap()).unwrap();
    let evaluated = dl8(&["eval", path.to_str().unwrap()], "ir:dd");
    assert_eq!(evaluated["diagnostics"], json!([]));
    assert_eq!(pick(&evaluated["closure"]), comptime);
    let expected: Vec<&str> = vec![
        "anyint: const(0)",
        "anyint: const(1)",
        "anyint: const(2)",
        "anyint: const(3)",
        "anyint: const(4)",
        "anyint: const(5)",
        "back: const(\"r\"), const(\"bb\")",
        "back: const(\"r\"), const(\"ccc\")",
        "back: const(\"s\"), const(5)",
        "counted: const(\"r\"), const(3)",
        "counted: const(\"s\"), const(2)",
        "counter: const(0)",
        "counter: const(1)",
        "counter: const(2)",
        "counter: const(3)",
        "counter: const(4)",
        "counter: const(5)",
        "diag: const(2)",
        "diag: const(3)",
        "diag: const(4)",
        "diag: const(5)",
        "each: const(\"r\"), const(0), const(\"a\")",
        "each: const(\"r\"), const(1), const(\"bb\")",
        "each: const(\"r\"), const(2), const(\"ccc\")",
        "each: const(\"s\"), const(0), const(4)",
        "each: const(\"s\"), const(1), const(5)",
        "files: const(\"r\"), ref(application(vec, [const(\"a\"), const(\"bb\"), const(\"ccc\")]))",
        "files: const(\"s\"), ref(application(vec, [const(4), const(5)]))",
        "fixed: const(42)",
        "lens: const(\"a\"), const(1)",
        "lens: const(\"bb\"), const(2)",
        "lens: const(\"ccc\"), const(3)",
        "loose: const(\"r\"), const(\"a\")",
        "loose: const(\"r\"), const(\"bb\")",
        "loose: const(\"r\"), const(\"ccc\")",
        "loose: const(\"s\"), const(4)",
        "loose: const(\"s\"), const(5)",
        "loose: const(\"z\"), const(7)",
        "lowest: const(\"r\"), const(2)",
        "lowest: const(\"s\"), const(1)",
        "next_index: const(\"r\"), const(1)",
        "next_index: const(\"r\"), const(2)",
        "next_index: const(\"r\"), const(3)",
        "next_index: const(\"s\"), const(1)",
        "next_index: const(\"s\"), const(2)",
        "pair: const(0), const(0)",
        "pair: const(1), const(1)",
        "pair: const(2), const(2)",
        "pair: const(3), const(3)",
        "pair: const(4), const(4)",
        "pair: const(5), const(5)",
        "size: const(\"r\"), const(3)",
        "size: const(\"s\"), const(2)",
        "small: const(\"r\"), const(0)",
        "small: const(\"r\"), const(1)",
        "small: const(\"s\"), const(0)",
        "small: const(\"s\"), const(1)",
        "summed: const(\"r\"), const(3)",
        "summed: const(\"s\"), const(1)",
        "texts: const(0), const(\"0\")",
        "texts: const(1), const(\"1\")",
        "texts: const(2), const(\"2\")",
        "texts: const(3), const(\"3\")",
        "texts: const(4), const(\"4\")",
        "texts: const(5), const(\"5\")",
        "three: const(3)",
    ];
    assert_eq!(comptime, expected);
}
