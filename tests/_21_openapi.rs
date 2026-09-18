//! `dl8 compile` through the real binary over `fixtures/openapi/todo.json`, a
//! hand rendering of `hafley-tsp/examples/todo-app.tsp`. The program imports
//! `@std/oai` and names its own document; no CLI flag seeds the module.
//!
//! FAIL-FIRST RECEIPT: drop the `optional` wrap in `fill_node` and
//! `mapping_rows_land_in_the_type_graph` fails at "Todo.body is not a sum";
//! point `route_fact` at no relation and every route seed disappears.

use serde_json::Value;
use std::collections::BTreeMap;
use std::path::{Path, PathBuf};
use std::process::Command;

const ROOT_TOKEN: &str = "/V8ROOT";
const OWNER: &str = "module(tsi('openapi',['/V8ROOT/fixtures/openapi/todo.json']))";

fn v8_root() -> PathBuf {
    let root = Path::new(env!("CARGO_MANIFEST_DIR"));
    std::fs::canonicalize(root).unwrap_or_else(|_| root.to_path_buf())
}

struct Compiled {
    code: Option<i32>,
    rows: Vec<Value>,
    diagnostics: Vec<Value>,
}

/// The program names its documents; the run is from the v8 root so the owner
/// scope is the same absolute path in every environment.
fn compile(program: &str) -> Compiled {
    let mut command = Command::new(env!("CARGO_BIN_EXE_dl8"));
    command
        .current_dir(v8_root())
        .args(["compile", &format!("fixtures/openapi/{program}.dl7")])
        .args(["--project", "fixtures/openapi"]);
    let output = command.output().expect("dl8 runs");
    let text = String::from_utf8(output.stdout)
        .expect("dl8 writes utf8")
        .replace(v8_root().to_str().expect("utf8 root"), ROOT_TOKEN);
    let compiled: Value = serde_json::from_str(&text).unwrap_or_else(|e| {
        panic!(
            "dl8 wrote no JSON ({e}): {}",
            String::from_utf8_lossy(&output.stderr)
        )
    });
    Compiled {
        code: output.status.code(),
        rows: compiled["compiler_rows"]
            .as_array()
            .cloned()
            .unwrap_or_default(),
        diagnostics: compiled["diagnostics"]
            .as_array()
            .cloned()
            .unwrap_or_default(),
    }
}

/// The `_6_eval::json` term encoding read back as prolog text.
fn render(term: &Value) -> String {
    match term {
        Value::Object(object) => {
            if let Some(Value::String(atom)) = object.get("a") {
                return format!("'{atom}'");
            }
            if let Some(Value::String(text)) = object.get("s") {
                return format!("\"{text}\"");
            }
            let functor = object["f"].as_str().expect("compound functor");
            let args: Vec<String> = object["args"]
                .as_array()
                .expect("compound args")
                .iter()
                .map(render)
                .collect();
            format!("{functor}({})", args.join(","))
        }
        Value::Array(items) => {
            let items: Vec<String> = items.iter().map(render).collect();
            format!("[{}]", items.join(","))
        }
        other => other.to_string(),
    }
}

fn frozen(name: &str) -> Value {
    let path = v8_root().join("fixtures/openapi").join(name);
    serde_json::from_str(&std::fs::read_to_string(&path).expect("frozen file is committed"))
        .expect("frozen file is JSON")
}

/// The callable `@std/oai` declares for one member, read off its own `:` edge
/// so the frozen rows never pin a reader node id.
fn oai_member(rows: &[Value], member: &str) -> String {
    let prefix = format!("call(ref(kernel(':')),[ref(module(std('oai'))),const('{member}'),ref(");
    let row = rows
        .iter()
        .map(render)
        .find(|row| row.starts_with(&prefix))
        .unwrap_or_else(|| panic!("no @std/oai edge for {member}"));
    let rest = &row[prefix.len()..];
    rest[..rest.rfind("),const(").expect("edge index")].to_string()
}

fn fixture_rows(compiled: &Compiled) -> Vec<Value> {
    let route = format!("call(ref({}),", oai_member(&compiled.rows, "route"));
    let param = format!("call(ref({}),", oai_member(&compiled.rows, "param"));
    compiled
        .rows
        .iter()
        .filter(|row| {
            let text = render(row);
            text.starts_with(&route)
                || text.starts_with(&param)
                || text.starts_with("call(ref(owner(file('/V8ROOT/fixtures/openapi/todo.dl7')")
        })
        .cloned()
        .collect()
}

#[test]
fn todo_routes_params_and_conforms_match_the_frozen_rows() {
    let compiled = compile("todo");
    assert!(
        compiled.diagnostics.is_empty(),
        "dl8 reported {:?}",
        compiled.diagnostics
    );
    assert_eq!(compiled.code, Some(0));
    let rows = fixture_rows(&compiled);
    let derived: Vec<String> = rows
        .iter()
        .map(render)
        .filter(|row| row.starts_with("call(ref(owner(file("))
        .map(|row| row[row.rfind('[').expect("argument list")..].to_string())
        .collect();
    assert_eq!(
        derived,
        [
            r#"[const("/todos"),const('post')])"#,
            r#"[const("/todos/{id}"),const('get')])"#,
        ],
        "route_returns_todo holds for the two routes answering one Todo"
    );
    assert_eq!(Value::Array(rows), frozen("expected_rows.json"));
}

/// `name -> node` from the `tsi.name` seeds, then one assertion per mapping row.
#[test]
fn mapping_rows_land_in_the_type_graph() {
    let compiled = compile("todo");
    assert!(
        compiled.diagnostics.is_empty(),
        "{:?}",
        compiled.diagnostics
    );
    let route_callable = oai_member(&compiled.rows, "route");
    let param_callable = oai_member(&compiled.rows, "param");
    let rows: Vec<String> = compiled
        .rows
        .iter()
        .map(|row| render(row).replace(OWNER, "OA"))
        .collect();

    let mut named: BTreeMap<String, String> = BTreeMap::new();
    let name_prefix = "call(ref(tsi_relation(OA,'tsi.name')),[ref(";
    for row in &rows {
        if let Some(rest) = row.strip_prefix(name_prefix) {
            let (node, name) = rest.split_once("),const(\"").expect("name seed shape");
            named.insert(name.trim_end_matches("\")])").to_string(), node.to_string());
        }
    }
    let node = |name: &str| {
        named
            .get(name)
            .cloned()
            .unwrap_or_else(|| panic!("no tsi.name row for {name}: {named:?}"))
    };
    let edge = |owner: &str, label: &str| -> String {
        let prefix = format!("call(ref(kernel(':')),[ref({owner}),const('{label}'),ref(");
        let row = rows
            .iter()
            .find(|row| row.starts_with(&prefix))
            .unwrap_or_else(|| panic!("no edge {label} on {owner}"));
        let rest = &row[prefix.len()..];
        rest[..rest.rfind("),const(").expect("edge index")].to_string()
    };
    let has = |fact: &str| rows.iter().any(|row| row == fact);
    let route = |operation: &str| -> String {
        rows.iter()
            .find(|row| {
                row.starts_with(&format!("call(ref({route_callable}),"))
                    && row.contains(&format!("const(\"{operation}\")"))
            })
            .unwrap_or_else(|| panic!("no route {operation}"))
            .clone()
    };
    let tsi = |class: &str| edge("module(std('tsi'))", class);

    let todo = node("Todo");
    assert!(
        has(&format!("call(ref(kernel('product')),[ref({todo})])")),
        "Todo is not a product"
    );

    let string = tsi("string");
    assert_eq!(
        edge(&todo, "title"),
        string,
        "a required string column is the @std/tsi class"
    );
    assert_eq!(
        edge(&todo, "id"),
        string,
        "$ref TodoId resolves through the alias to string"
    );

    let body = edge(&todo, "body");
    assert!(
        has(&format!("call(ref(kernel('sum')),[ref({body})])")),
        "Todo.body is not a sum"
    );
    assert_eq!(edge(&body, "value"), string);
    assert_eq!(edge(&body, "null"), tsi("null"));
    assert_eq!(
        edge(&todo, "completedAt"),
        body,
        "type [string, null] is the same option node as an unrequired string"
    );

    let priority = node("Priority");
    assert!(has(&format!("call(ref(kernel('sum')),[ref({priority})])")));
    for value in ["low", "medium", "high", "urgent"] {
        assert_eq!(edge(&priority, value), node(&format!("Priority::{value}")));
    }

    let lookup = node("TodoLookup");
    assert!(has(&format!("call(ref(kernel('sum')),[ref({lookup})])")));
    assert_eq!(
        edge(&lookup, "found"),
        todo,
        "discriminator mapping labels the branch"
    );
    assert_eq!(edge(&lookup, "missing"), node("NotFound"));

    let array_of_todo = format!("ref(application({},[{todo}]))", node("Array"));
    assert!(route("Todo_query_list").ends_with(&format!("{array_of_todo}])")));

    let i32_class = tsi("i32");
    assert!(route("Todo_query_count").ends_with(&format!("ref({i32_class})])")));
    assert!(
        route("Todo_query_get").contains(&format!("ref({})", tsi("void"))),
        "an operation with no request body takes void"
    );
    assert!(has(&format!(
        "call(ref({param_callable}),[const(\"Todo_query_get\"),const(\"id\"),const('path'),ref({string}),const('true')])"
    )));
}

#[test]
fn dangling_ref_is_a_diagnostic_naming_the_pointer() {
    let compiled = compile("dangling");
    assert_eq!(compiled.code, Some(1));
    assert_eq!(
        Value::Array(compiled.diagnostics),
        frozen("expected_dangling_diagnostics.json")
    );
}

#[test]
fn schema_declared_by_two_documents_is_a_diagnostic() {
    let compiled = compile("duplicate");
    assert_eq!(compiled.code, Some(1));
    let rendered: Vec<String> = compiled.diagnostics.iter().map(render).collect();
    assert_eq!(
        rendered,
        ["diagnostic('openapi',document('/V8ROOT/fixtures/openapi/duplicate.json'),openapi_duplicate_schema(\"Todo\"))"]
    );
}
