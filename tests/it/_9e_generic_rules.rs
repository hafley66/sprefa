//! Generic product application in rules (plans/v8/2026-09-30-generic-rules
//! proposal, proofs P1-P5) through the public compiler on all five engines.

use serde_json::{json, Value};
use std::collections::BTreeSet;
use std::path::Path;
use std::process::Command;

pub const ENGINES: [&str; 5] = ["rust", "dd", "sqlite", "ir:dd", "ir:sqlite"];

/// A term as text: declared names for their owner nodes, no file paths.
pub fn render(term: &Value, names: &Value) -> String {
    let is_owner = term.get("f").and_then(Value::as_str) == Some("owner");
    let named = names.as_object().filter(|_| is_owner).and_then(|names| {
        names
            .iter()
            .find(|(_, value)| value["args"][0] == *term)
            .map(|(name, _)| name.clone())
    });
    if let Some(name) = named {
        return name;
    }
    if let Some(atom) = term.get("a").and_then(Value::as_str) {
        return atom.to_owned();
    }
    if let Some(text) = term.get("s").and_then(Value::as_str) {
        return format!("{text:?}");
    }
    if let Some(items) = term.as_array() {
        let items: Vec<String> = items.iter().map(|item| render(item, names)).collect();
        return format!("[{}]", items.join(", "));
    }
    let Some(functor) = term.get("f").and_then(Value::as_str) else {
        return term.to_string();
    };
    let args = term["args"].as_array().unwrap();
    match functor {
        "file" => "file".to_owned(),
        "reader_node" => format!("reader_node({})", args[1]),
        _ => {
            let args: Vec<String> = args.iter().map(|arg| render(arg, names)).collect();
            format!("{functor}({})", args.join(", "))
        }
    }
}

/// `rel: arg, arg ..` for a closure row or a compiler `call` row.
pub fn render_row(row: &Value, names: &Value) -> String {
    let (relation, args) = match row.get("rel") {
        Some(relation) => (relation, &row["args"]),
        None => (&row["args"][0], &row["args"][1]),
    };
    let relation = render(&relation["args"][0], names);
    let args: Vec<String> = args
        .as_array()
        .unwrap()
        .iter()
        .map(|arg| render(arg, names))
        .collect();
    format!("{relation}: {}", args.join(", "))
}

/// The rendered rows of `relations` that mention `needle`, sorted.
fn select(rows: &Value, names: &Value, relations: &[&str], needle: &str) -> Vec<String> {
    let rendered: BTreeSet<String> = rows
        .as_array()
        .unwrap()
        .iter()
        .map(|row| render_row(row, names))
        .filter(|row| relations.iter().any(|relation| row.starts_with(&format!("{relation}: "))))
        .filter(|row| row.contains(needle))
        .collect();
    rendered.into_iter().collect()
}

fn compile(fixture: &str, engine: &str) -> Value {
    let root = Path::new(env!("CARGO_MANIFEST_DIR"));
    let compiled = Command::new(env!("CARGO_BIN_EXE_dl8"))
        .arg("compile")
        .arg(root.join("fixtures/generic").join(fixture))
        .env("DL8_ENGINE", engine)
        .output()
        .unwrap();
    assert!(
        compiled.status.success(),
        "{engine}: {}",
        String::from_utf8_lossy(&compiled.stderr)
    );
    let program: Value = serde_json::from_slice(&compiled.stdout).unwrap();
    assert_eq!(program["diagnostics"], json!([]), "{engine}");
    program
}

fn scratch(tag: &str) -> std::path::PathBuf {
    let root = Path::new(env!("CARGO_MANIFEST_DIR"));
    let directory = root.join(format!("target/generic-rules-{tag}-{}", std::process::id()));
    std::fs::create_dir_all(&directory).unwrap();
    directory
}

fn eval(program: &Value, directory: &Path, engine: &str) -> Value {
    let path = directory.join("program.json");
    std::fs::write(&path, serde_json::to_vec(program).unwrap()).unwrap();
    let evaluated = Command::new(env!("CARGO_BIN_EXE_dl8"))
        .arg("eval")
        .arg(&path)
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
    closure
}

/// P1, P2, P3 at comptime: `boxed_column` builds `(Box ?ColumnType)` from the
/// type graph, the binds build `(Box str)` and `(Box bool)`, and
/// `box_item_type` reads every instance back. `(Box str)` is one node with one
/// `item` edge whichever side built it.
#[test]
fn box_rules_build_bind_and_read_back_at_comptime() {
    let expected = vec![
        "box_item_type: ref(application(Box, [primitive(bool)])), ref(primitive(bool))",
        "box_item_type: ref(application(Box, [primitive(int)])), ref(primitive(int))",
        "box_item_type: ref(application(Box, [primitive(str)])), ref(primitive(str))",
        "boxed_column: ref(Product), const(count), ref(application(Box, [primitive(int)]))",
        "boxed_column: ref(Product), const(name), ref(application(Box, [primitive(str)]))",
        "kernel(:): ref(application(Box, [primitive(bool)])), const(item), ref(primitive(bool)), const(0)",
        "kernel(:): ref(application(Box, [primitive(int)])), const(item), ref(primitive(int)), const(0)",
        "kernel(:): ref(application(Box, [primitive(str)])), const(item), ref(primitive(str)), const(0)",
        "kernel(:): ref(module(file)), const(Flagged), ref(application(Box, [primitive(bool)])), const(3)",
        "kernel(:): ref(module(file)), const(Wrapped), ref(application(Box, [primitive(str)])), const(2)",
    ];
    for engine in ENGINES {
        let program = compile("4_box_rules.dl7", engine);
        let names = &program["program"]["names"];
        let rows = select(
            &program["compiler_rows"],
            names,
            &["kernel(:)", "boxed_column", "box_item_type"],
            "application(Box",
        );
        assert_eq!(rows, expected, "{engine}");
    }
}

/// P1, P2 and P4 at runtime: the rule-built instances are rule output. `(Box
/// int)` from the bind and from the `count` row is one `intern` row;
/// retracting the `name` row takes the `(Box str)` rows with it and leaves the
/// bind's.
#[test]
fn box_rule_rows_follow_their_source_row() {
    let before = vec![
        "boxed_column: const(\"Product\"), const(\"count\"), ref(application(Box, [primitive(int)]))",
        "boxed_column: const(\"Product\"), const(\"name\"), ref(application(Box, [primitive(str)]))",
        "kernel(:): ref(application(Box, [primitive(int)])), const(item), ref(primitive(int)), const(0)",
        "kernel(:): ref(application(Box, [primitive(str)])), const(item), ref(primitive(str)), const(0)",
        "kernel(intern): ref(Box), ref(application(tuple, [ref(primitive(int))])), ref(application(Box, [primitive(int)]))",
        "kernel(intern): ref(Box), ref(application(tuple, [ref(primitive(str))])), ref(application(Box, [primitive(str)]))",
        "kernel(intern_snapshot): ref(Box), ref(application(tuple, [ref(primitive(int))])), ref(application(Box, [primitive(int)]))",
    ];
    let after: Vec<&str> = before
        .iter()
        .copied()
        .filter(|row| !row.contains("primitive(str)"))
        .collect();
    let relations = ["kernel(:)", "kernel(intern)", "kernel(intern_snapshot)", "boxed_column"];
    let retracted = json!({"f": "const", "args": [{"s": "name"}]});
    let directory = scratch("retract");
    let frontiers = directory.join("frontiers.json");
    std::fs::write(
        &frontiers,
        serde_json::to_vec(&json!([{"changes": [{"rel": "product_column", "args": [
            {"f": "const", "args": [{"s": "Product"}]},
            retracted,
            {"f": "ref", "args": [{"f": "primitive", "args": [{"a": "str"}]}]}
        ], "w": -1}]}]))
        .unwrap(),
    )
    .unwrap();
    for engine in ENGINES {
        let program = compile("5_box_retract.dl7", engine);
        let names = &program["program"]["names"];
        let closure = eval(&program, &directory, engine);
        let rows = select(&closure["closure"], names, &relations, "application(Box");
        assert_eq!(rows, before, "{engine} before");

        let after_rows = if engine.starts_with("ir:") {
            let run = Command::new(env!("CARGO_BIN_EXE_dl8"))
                .arg("run")
                .arg(directory.join("program.json"))
                .arg("--frontiers")
                .arg(&frontiers)
                .env("DL8_ENGINE", engine)
                .output()
                .unwrap();
            assert!(run.status.success(), "{engine}: {}", String::from_utf8_lossy(&run.stderr));
            let run: Value = serde_json::from_slice(&run.stdout).unwrap();
            assert_eq!(run["diagnostics"], json!([]), "{engine}");
            let closure = &run["frontiers"][0]["closure"];
            assert_eq!(closure["diagnostics"], json!([]), "{engine}");
            select(&closure["closure"], names, &relations, "application(Box")
        } else {
            // The engines without frontiers evaluate the program that never
            // held the row.
            let mut without = program.clone();
            let relation = names["product_column"].clone();
            without["program"]["seeds"]
                .as_array_mut()
                .unwrap()
                .retain(|seed| !(seed["rel"] == relation && seed["args"][1] == retracted));
            let closure = eval(&without, &directory, engine);
            select(&closure["closure"], names, &relations, "application(Box")
        };
        assert_eq!(after_rows, after, "{engine} after");
    }
    std::fs::remove_dir_all(directory).unwrap();
}

/// P5: `(EResult ?Next ?Error)` in a rule head with both `str` is the node the
/// bind `(FetchStatus: (EResult str str))` names, with the same variant edges
/// and the same `intern` rows.
#[test]
fn eresult_in_a_rule_is_the_bound_instance() {
    let directory = scratch("eresult");
    for engine in ENGINES {
        let program = compile("6_eresult_rule.dl7", engine);
        let names = &program["program"]["names"];
        let bound = select(&program["compiler_rows"], names, &["kernel(:)"], "const(FetchStatus)");
        let [bound] = &bound[..] else {
            panic!("{engine}: {bound:?}");
        };
        let instance = bound
            .strip_prefix("kernel(:): ref(module(file)), const(FetchStatus), ")
            .and_then(|rest| rest.strip_suffix(", const(1)"))
            .unwrap();
        assert!(instance.starts_with("ref(application(EResult, "), "{engine}: {instance}");

        let closure = eval(&program, &directory, engine);
        let built = select(&closure["closure"], names, &["status_type"], "application");
        assert_eq!(built, vec![format!("status_type: {instance}")], "{engine}");

        let arguments = "[primitive(str), primitive(str)])";
        let bound_edges = select(&program["compiler_rows"], names, &["kernel(:)"], arguments);
        let bound_edges: Vec<String> =
            bound_edges.into_iter().filter(|row| !row.contains("FetchStatus")).collect();
        let built_edges = select(&closure["closure"], names, &["kernel(:)"], arguments);
        assert_eq!(built_edges.len(), 6, "{engine}");
        assert_eq!(built_edges, bound_edges, "{engine}");
        let interns = select(&closure["closure"], names, &["kernel(intern)"], arguments);
        let snapshots = select(&closure["closure"], names, &["kernel(intern_snapshot)"], arguments);
        assert_eq!(interns.len(), 5, "{engine}");
        let snapshots: Vec<String> = snapshots
            .iter()
            .map(|row| row.replacen("kernel(intern_snapshot)", "kernel(intern)", 1))
            .collect();
        assert_eq!(interns, snapshots, "{engine}");
    }
    std::fs::remove_dir_all(directory).unwrap();
}
