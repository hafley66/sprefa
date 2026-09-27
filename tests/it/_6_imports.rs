use serde_json::Value;
use std::collections::BTreeMap;
use std::path::Path;
use std::path::PathBuf;
use std::process::Command;
use std::time::Instant;

// From tests/_16_extract_tsi.rs
// `sprefa-extract`, now living in `hafley-rs`, through its real binary into
// `dl8 compile --tsi`. Three runs over `fixtures/extract/corpus`: the TSI
// envelope, resolved edges, and `ryi scip`. The first run's JSONL is
// the stream `dl8` loads, and the compiler rows it produces are compared
// against `fixtures/extract/expected_tsi_rows.json` on the rows that name the
// corpus.
//
// FAIL-FIRST RECEIPT: drop `--witness` from `tsi_stream` and the same run
// prints `resolved_import` records and no `protocol` record, so
// `tsi_envelope_carries_the_corpus_type_graph` fails at "the stream opens
// with no protocol record" and `dl8_compile_consumes_the_extracted_stream`
// finds no compiler row naming the corpus.
//
// `tsi` is the envelope `--witness` wraps around `--kinds type`.
// Several paths in one run need `--resolve` for cross-file edges.

const MISSING_EXTRACT_REASON: &str =
    "ryi executable is absent; hafley-rs now declares the external extractor binary as ryii";

fn v8_root() -> PathBuf {
    Path::new(env!("CARGO_MANIFEST_DIR")).to_path_buf()
}

fn extract_roots() -> [PathBuf; 2] {
    let root = v8_root();
    let parent = root
        .parent()
        .expect("v8 has a parent directory")
        .to_path_buf();
    [root, parent]
}

fn fixture_root() -> PathBuf {
    v8_root().join("fixtures/extract")
}

/// The corpus paths in one order, so two runs name the same files. Relative to
/// the v8 root, because `resolved_import` records carry the path as given and
/// an absolute one would pin this checkout into the stream.
fn corpus_paths() -> Vec<PathBuf> {
    [
        "fixtures/extract/corpus/ts/records.ts",
        "fixtures/extract/corpus/ts/format.ts",
        "fixtures/extract/corpus/ts/report.ts",
        "fixtures/extract/corpus/rust/shapes.rs",
        "fixtures/extract/corpus/rust/report.rs",
    ]
    .iter()
    .map(PathBuf::from)
    .collect()
}

fn scratch(name: &str) -> PathBuf {
    let path = std::env::temp_dir().join(format!("dl8-extract-{}-{name}", std::process::id()));
    if let Some(parent) = path.parent() {
        std::fs::create_dir_all(parent).expect("scratch parent");
    }
    path
}

/// Where a built `ryi` can sit. `$CARGO_TARGET_DIR` comes first because a
/// lane sets it, and then neither target directory ever fills. The workspace
/// one is second: the crate was a member until hafley-rs excluded it.
fn built_candidates() -> Vec<PathBuf> {
    let mut out = Vec::new();
    if let Some(shared) = std::env::var_os("CARGO_TARGET_DIR") {
        out.push(PathBuf::from(shared).join("debug/ryi"));
    }
    for root in extract_roots() {
        out.push(root.join("hafley-rs/target/debug/ryi"));
        out.push(root.join("hafley-rs/crates/sprefa-extract/target/debug/ryi"));
    }
    out
}

/// A built binary in the worktree or its parent, then `$SPREFA_EXTRACT_BIN`.
/// The external crate is never built by this test suite.
fn extract_bin() -> Option<PathBuf> {
    if let Some(built) = built_candidates().into_iter().find(|path| path.exists()) {
        return Some(built);
    }
    if let Some(named) = std::env::var_os("SPREFA_EXTRACT_BIN") {
        let path = PathBuf::from(named);
        return path.is_file().then_some(path);
    }
    None
}

fn skip_without_extract() -> bool {
    if extract_bin().is_none() {
        println!("SKIP: {MISSING_EXTRACT_REASON}");
        return true;
    }
    false
}

struct Run {
    stdout: String,
    stderr: String,
    millis: u128,
}

fn extract_run(args: &[&str], paths: &[PathBuf]) -> Run {
    let mut command = Command::new(extract_bin().expect("extract availability checked by test"));
    command.current_dir(v8_root()).args(args).args(paths);
    let start = Instant::now();
    let output = command.output().expect("extract runs");
    let millis = start.elapsed().as_millis();
    assert!(
        output.status.success(),
        "extract {args:?} exited {:?}: {}",
        output.status.code(),
        String::from_utf8_lossy(&output.stderr)
    );
    Run {
        stdout: String::from_utf8(output.stdout).expect("extract writes utf8"),
        stderr: String::from_utf8_lossy(&output.stderr).to_string(),
        millis,
    }
}

fn records(stream: &str) -> Vec<Value> {
    stream
        .lines()
        .filter(|line| !line.trim().is_empty())
        .map(|line| serde_json::from_str(line).expect("extract writes one JSON object per line"))
        .collect()
}

fn record_kind(row: &Value, kind: &str) -> bool {
    row.get("record").and_then(Value::as_str) == Some(kind)
}

fn relation_of(row: &Value) -> Option<&str> {
    if !record_kind(row, "fact") {
        return None;
    }
    row.get("relation").and_then(Value::as_str)
}

/// The names the corpus declares. A compiler row carrying one of these came
/// from the extracted stream and not from the prelude.
const CORPUS_NAMES: [&str; 7] = [
    "UserRecord",
    "TeamRecord",
    "formatUser",
    "formatTeam",
    "reportTeam",
    "format_user",
    "format_team",
];

fn names_corpus(row: &Value) -> bool {
    let text = serde_json::to_string(row).expect("a compiler row re-encodes");
    CORPUS_NAMES
        .iter()
        .any(|name| text.contains(&format!("\"{name}\"")))
}

fn tsi_stream() -> (PathBuf, Run) {
    let run = extract_run(
        &[
            "--witness",
            "--resolve",
            "--arms",
            "type",
            "--kinds",
            "type",
        ],
        &corpus_paths(),
    );
    let path = scratch("corpus.tsi.jsonl");
    std::fs::write(&path, &run.stdout).expect("scratch stream");
    (path, run)
}

#[test]
fn tsi_envelope_carries_the_corpus_type_graph() {
    if skip_without_extract() {
        return;
    }
    let (_, run) = tsi_stream();
    let rows = records(&run.stdout);
    assert!(
        rows.first().is_some_and(|row| record_kind(row, "protocol")),
        "the stream opens with no protocol record"
    );
    assert!(
        rows.iter().any(|row| record_kind(row, "run")),
        "the stream carries no run record, so the loader has no owner"
    );
    let facts = rows.iter().filter_map(relation_of).count();
    assert!(facts > 0, "--kinds type emitted no relation rows");
    for relation in ["tsi.type", "tsi.product", "tsi.edge", "tsi.name"] {
        assert!(
            rows.iter().any(|row| relation_of(row) == Some(relation)),
            "no {relation} row over the corpus"
        );
    }
    assert!(
        rows.iter()
            .filter(|row| relation_of(row) == Some("tsi.name"))
            .any(names_corpus),
        "no tsi.name row names a corpus type"
    );
    assert!(
        rows.iter().any(|row| record_kind(row, "witness")),
        "the envelope carries no witness"
    );
    println!("tsi: {facts} relation rows, {} ms", run.millis);
}

#[test]
fn diet_scip_resolves_across_corpus_files() {
    if skip_without_extract() {
        return;
    }
    let run = extract_run(&["--resolve", "--arms", "call,type"], &corpus_paths());
    let rows = records(&run.stdout);
    assert!(!rows.is_empty(), "--resolve emitted no rows");
    for kind in ["resolved_edge", "resolved_type_edge", "resolved_import"] {
        assert!(
            rows.iter().any(|row| record_kind(row, kind)),
            "--resolve emitted no {kind} row over the corpus: {:?}",
            rows.iter()
                .filter_map(|row| row.get("record"))
                .collect::<Vec<_>>()
        );
    }
    let cross_file = rows.iter().any(|row| {
        let ends = [
            ("caller_path", "callee_path"),
            ("owner_path", "target_path"),
            ("src_path", "target_path"),
        ];
        ends.iter().any(|(from, to)| {
            let from = row.get(from).and_then(Value::as_str);
            let to = row.get(to).and_then(Value::as_str);
            matches!((from, to), (Some(from), Some(to)) if from != to)
        })
    });
    assert!(
        cross_file,
        "every diet_scip edge stayed inside one file; the corpus imports across files"
    );
    println!("diet_scip: {} rows, {} ms", rows.len(), run.millis);
}

#[test]
fn scip_indexes_the_typescript_corpus() {
    if skip_without_extract() {
        return;
    }
    if which("scip-typescript").is_none() {
        println!("ignored: scip-typescript is not on PATH, so ryi scip has no indexer to run");
        return;
    }
    // The index is a build artifact; the fixture tree stays committable.
    let cache = scratch("scip-cache");
    std::fs::create_dir_all(&cache).expect("scip cache directory");
    let run = extract_run(
        &[
            "scip",
            "--scip-cache",
            cache.to_str().expect("utf8 cache path"),
        ],
        &[PathBuf::from("fixtures/extract/corpus/ts")],
    );
    let rows = records(&run.stdout);
    let skipped: Vec<&Value> = rows
        .iter()
        .filter(|row| record_kind(row, "scip_skip"))
        .collect();
    assert!(
        skipped.is_empty(),
        "ryi scip skipped the corpus: {skipped:?}; stderr: {}",
        run.stderr
    );
    for kind in ["scip_def", "scip_ref", "scip_name"] {
        assert!(
            rows.iter().any(|row| record_kind(row, kind)),
            "no {kind} row over the TypeScript corpus"
        );
    }
    assert!(
        rows.iter()
            .filter(|row| record_kind(row, "scip_name"))
            .any(names_corpus),
        "no scip_name row names a corpus symbol"
    );
    println!("scip: {} rows, {} ms", rows.len(), run.millis);
}

#[test]
fn dl8_compile_consumes_the_extracted_stream() {
    if skip_without_extract() {
        return;
    }
    let (stream, _) = tsi_stream();
    let program = fixture_root().join("main.dl7");
    let output = Command::new(env!("CARGO_BIN_EXE_dl8"))
        .arg("compile")
        .arg(&program)
        .arg("--project")
        .arg(fixture_root())
        .arg("--tsi")
        .arg(&stream)
        .output()
        .expect("dl8 runs");
    assert!(
        output.status.success(),
        "dl8 compile --tsi exited {:?}: {}",
        output.status.code(),
        String::from_utf8_lossy(&output.stderr)
    );
    let compiled: Value =
        serde_json::from_slice(&output.stdout).expect("dl8 writes JSON on stdout");
    let diagnostics = compiled["diagnostics"]
        .as_array()
        .expect("diagnostics is an array");
    assert!(diagnostics.is_empty(), "dl8 reported {diagnostics:?}");
    let rows = compiled["compiler_rows"]
        .as_array()
        .expect("compiler_rows is an array");
    let named: Vec<Value> = rows
        .iter()
        .filter(|row| names_corpus(row))
        .cloned()
        .collect();
    assert!(
        !named.is_empty(),
        "no compiler row names a corpus type; the TSI stream did not reach the compiler"
    );
    let expected_path = fixture_root().join("expected_tsi_rows.json");
    let expected: Value = serde_json::from_str(
        &std::fs::read_to_string(&expected_path).expect("expected rows are committed"),
    )
    .expect("expected rows are JSON");
    assert_eq!(
        Value::Array(named.clone()),
        expected,
        "the rows naming the corpus differ from {}",
        expected_path.display()
    );
    println!(
        "dl8: {} compiler rows, {} name the corpus",
        rows.len(),
        named.len()
    );
}

/// `Command::new` on a missing program fails at spawn, so the scip case asks
/// first and says why it stood down.
fn which(program: &str) -> Option<PathBuf> {
    let path = std::env::var_os("PATH")?;
    std::env::split_paths(&path)
        .map(|directory| directory.join(program))
        .find(|candidate| candidate.is_file())
}

// From tests/_21_openapi.rs
// `dl8 compile` through the real binary over `fixtures/openapi/todo.json`, a
// hand rendering of `hafley-tsp/examples/todo-app.tsp`. The program imports
// `@std/oai` and names its own document; no CLI flag seeds the module.
//
// FAIL-FIRST RECEIPT: drop the `optional` wrap in `fill_node` and
// `mapping_rows_land_in_the_type_graph` fails at "Todo.body is not a sum";
// point `route_fact` at no relation and every route seed disappears.

const ROOT_TOKEN: &str = "/V8ROOT";
const OWNER: &str = "module(tsi('openapi',['/V8ROOT/fixtures/openapi/todo.json']))";

fn s21openapi_v8_root() -> PathBuf {
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
        .current_dir(s21openapi_v8_root())
        .args(["compile", &format!("fixtures/openapi/{program}.dl7")])
        .args(["--project", "fixtures/openapi"]);
    let output = command.output().expect("dl8 runs");
    let text = String::from_utf8(output.stdout)
        .expect("dl8 writes utf8")
        .replace(
            s21openapi_v8_root().to_str().expect("utf8 root"),
            ROOT_TOKEN,
        );
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
    let path = s21openapi_v8_root().join("fixtures/openapi").join(name);
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
