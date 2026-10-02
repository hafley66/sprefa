use dl8::_6_eval::json::term_from_json;
use dl8::_6_eval::json::term_to_json;
use dl8::_6_eval::TermId;
use dl8::_6_eval::Universe;
use dl8::_9_runtime::open;
use dl8::_9_runtime::sql;
use dl8::_9_runtime::IRowStore;
use dl8::_9_runtime::SqliteRowStore;
use dl8::_9_runtime::Watermark;
use rusqlite::Connection;
use serde_json::json;
use serde_json::Value;
use std::collections::BTreeMap;
use std::path::Path;
use std::path::PathBuf;
use std::process::Command;
use std::sync::mpsc;
use std::time::Instant;
use tracing_subscriber::layer::SubscriberExt;

// From tests/_13_store.rs
// The closure at rest, through the real binary. Each `fixtures/store/*.dl7`
// compiles to a checked runtime program, which `dl8 eval --db` evaluates into
// one SQLite file; a second run against the same file loads the arena and the
// rows back and derives only what the new seeds add. The fixture path is
// replaced by `<fixture>` so the expected files carry no absolute path.
//
// `dl8 compile`'s whole stdout is the program file; `dl8 eval` reads its
// `program` key, and the store names each table after the declared relation.

fn normalize(value: &Value, path: &str) -> Value {
    match value {
        Value::String(s) => Value::String(s.replace(path, "<fixture>")),
        Value::Array(items) => Value::Array(items.iter().map(|v| normalize(v, path)).collect()),
        Value::Object(map) => Value::Object(
            map.iter()
                .map(|(k, v)| (k.clone(), normalize(v, path)))
                .collect(),
        ),
        other => other.clone(),
    }
}

fn fixture(name: &str) -> PathBuf {
    Path::new(env!("CARGO_MANIFEST_DIR"))
        .join("fixtures/store")
        .join(name)
}

/// One directory per test, so two tests never share a db file.
fn scratch(name: &str) -> PathBuf {
    let directory = std::env::temp_dir().join(format!("dl8-store-{}-{name}", std::process::id()));
    let _ = std::fs::remove_dir_all(&directory);
    std::fs::create_dir_all(&directory).unwrap();
    directory
}

fn expected(name: &str) -> Value {
    let path = fixture(&format!("{name}.expected.json"));
    let text = std::fs::read_to_string(&path).unwrap_or_else(|e| panic!("{}: {e}", path.display()));
    serde_json::from_str(&text).unwrap()
}

struct Run {
    closure: Value,
    events: Vec<Value>,
}

/// `program`'s file stem becomes the store's table prefix, so two runs that
/// continue one program write the same JSON file name.
fn transport(source: &Path, program: &Path) {
    let output = Command::new(env!("CARGO_BIN_EXE_dl8"))
        .arg("compile")
        .arg(source)
        .output()
        .unwrap();
    let compiled: Value = serde_json::from_slice(&output.stdout).unwrap_or_else(|e| {
        panic!(
            "compile no JSON ({e}); stderr: {}",
            String::from_utf8_lossy(&output.stderr)
        )
    });
    assert_eq!(compiled["diagnostics"], json!([]), "compile diagnostics");
    std::fs::write(program, &output.stdout).unwrap();
}

fn evaluate(program: &Path, db: &Path, source: &Path) -> Run {
    let output = Command::new(env!("CARGO_BIN_EXE_dl8"))
        .args(["eval", "--db"])
        .arg(db)
        .arg(program)
        .env("RUST_LOG", "dl8=info")
        .env("HAFLEY_LOG_FORMAT", "json")
        .output()
        .unwrap();
    let stderr = String::from_utf8(output.stderr).unwrap();
    assert_eq!(output.status.code(), Some(0), "eval failed:\n{stderr}");
    let closure: Value = serde_json::from_slice(&output.stdout)
        .unwrap_or_else(|e| panic!("eval no JSON ({e}); stderr: {stderr}"));
    let source = std::fs::canonicalize(source).unwrap().display().to_string();
    Run {
        closure: normalize(&closure, &source),
        events: stderr
            .lines()
            .filter_map(|line| serde_json::from_str::<Value>(line).ok())
            .filter(|value| value["target"] == "dl8::store")
            .collect(),
    }
}

fn inserts(run: &Run) -> Vec<(String, u64)> {
    run.events
        .iter()
        .filter(|value| value["fields"]["statement"] == "insert")
        .map(|value| {
            (
                value["fields"]["table"].as_str().unwrap().to_string(),
                value["fields"]["rows"].as_u64().unwrap(),
            )
        })
        .collect()
}

/// Every product table of one program, as `(table, arity)`. An unnamed
/// relation's table carries its arena id, which no fixture may spell out.
fn dictionary(db: &Path, program: &str) -> Vec<(String, i64)> {
    let connection = open(db).unwrap();
    let mut statement = connection
        .prepare(&format!(
            "SELECT \"rel\",\"arity\",\"name\" FROM \"{program}.relation\" ORDER BY \"__id\""
        ))
        .unwrap();
    statement
        .query_map((), |row| {
            let rel: i64 = row.get(0)?;
            let arity: i64 = row.get(1)?;
            let name: Option<String> = row.get(2)?;
            let object = match name {
                Some(name) => format!("{name}_a{arity}"),
                None => format!("rel{rel}_a{arity}"),
            };
            Ok((format!("{program}.{object}"), arity))
        })
        .unwrap()
        .map(Result::unwrap)
        .collect()
}

/// Product tables as `(arity, rows)`.
fn products(db: &Path, program: &str) -> Vec<(i64, i64)> {
    let connection = open(db).unwrap();
    let mut out: Vec<(i64, i64)> = dictionary(db, program)
        .iter()
        .map(|(table, arity)| {
            let rows = connection
                .query_row(&format!("SELECT count(*) FROM \"{table}\""), (), |row| {
                    row.get(0)
                })
                .unwrap();
            (*arity, rows)
        })
        .collect();
    out.sort();
    out
}

/// Table names SQLite itself reports, so a named table that was never created
/// cannot pass by sitting in the dictionary alone.
fn sqlite_master(db: &Path) -> Vec<String> {
    let connection = open(db).unwrap();
    let mut statement = connection
        .prepare("SELECT \"name\" FROM sqlite_master WHERE \"type\" = 'table' ORDER BY \"name\"")
        .unwrap();
    let mut out: Vec<String> = statement
        .query_map((), |row| row.get(0))
        .unwrap()
        .map(Result::unwrap)
        .collect();
    out.sort();
    out
}

#[test]
pub fn arena_round_trip_preserves_every_id() {
    let directory = scratch("arena");
    let file = directory.join("arena.db");
    let mut u = Universe::new();
    let integers: Vec<TermId> = (0..5).map(|n| u.int(n)).collect();
    let floats: Vec<TermId> = (0..5).map(|n| u.float(n as f64 + 0.25)).collect();
    let booleans = vec![u.boolean(true), u.boolean(false)];
    let atoms: Vec<TermId> = (0..5).map(|n| u.atom(&format!("atom_{n}"))).collect();
    let strings: Vec<TermId> = (0..5).map(|n| u.string(&format!("text {n}"))).collect();
    let compounds: Vec<TermId> = (0..5)
        .map(|n| u.compound("wrap", vec![integers[n], strings[n], floats[n]]))
        .collect();

    let mut store = SqliteRowStore::at(&file).unwrap();
    store.open("arena").unwrap();
    store.begin_tick().unwrap();
    let written = store.commit_arena(&u, Watermark::default()).unwrap();
    store.commit_tick().unwrap();
    drop(store);
    assert_eq!(written.terms, u.terms.len());
    assert_eq!(written.syms, u.syms.len());

    let mut back = Universe::new();
    let mut reopened = SqliteRowStore::at(&file).unwrap();
    reopened.open("arena").unwrap();
    let loaded = reopened.load_arena(&mut back).unwrap();
    assert_eq!(loaded, written, "watermark differs after reload");
    let every: Vec<TermId> = [integers, floats, booleans, atoms, strings, compounds].concat();
    assert_eq!(every.len(), 27);
    for id in every {
        assert_eq!(
            back.display(id).to_string(),
            u.display(id).to_string(),
            "term {} differs after reload",
            id.0
        );
    }
    let _ = std::fs::remove_dir_all(&directory);
}

#[test]
pub fn reopening_the_db_leaves_the_closure_identical() {
    let directory = scratch("round-trip");
    let source = fixture("0_round_trip.dl7");
    let program = directory.join("0_round_trip.json");
    let db = directory.join("store.db");
    transport(&source, &program);

    let first = evaluate(&program, &db, &source);
    assert_eq!(
        first.closure["closure"],
        expected("0_round_trip")["closure"],
        "first run closure"
    );
    assert_eq!(first.closure["diagnostics"], json!([]));
    let counted = vec![(2, 2), (4, 2)];
    assert_eq!(products(&db, "0_round_trip"), counted, "Copied and Reading");

    let second = evaluate(&program, &db, &source);
    // IR engines also print work counters; the reloaded run seeds the stored rows, so its work differs.
    for field in ["closure", "diagnostics"] {
        assert_eq!(
            second.closure[field], first.closure[field],
            "{field} differs on the reloaded run"
        );
    }
    assert_eq!(products(&db, "0_round_trip"), counted, "rows rewritten");
    assert_eq!(inserts(&second), Vec::new(), "the reloaded run wrote rows");
    let _ = std::fs::remove_dir_all(&directory);
}

#[test]
pub fn continuing_a_program_writes_only_the_new_rows() {
    let directory = scratch("continue");
    let source = directory.join("1_continue.dl7");
    let program = directory.join("1_continue.json");
    let db = directory.join("store.db");

    std::fs::copy(fixture("1_continue.dl7"), &source).unwrap();
    transport(&source, &program);
    let first = evaluate(&program, &db, &source);
    assert_eq!(first.closure["diagnostics"], json!([]));

    std::fs::copy(fixture("1_continue_more.dl7"), &source).unwrap();
    transport(&source, &program);
    let second = evaluate(&program, &db, &source);
    assert_eq!(
        second.closure["closure"],
        expected("1_continue_more")["closure"],
        "continued closure"
    );

    let inserts = inserts(&second);
    assert!(
        inserts.len() <= 4,
        "the continued run executed {} INSERT statements: {inserts:?}",
        inserts.len()
    );
    let rows: u64 = inserts
        .iter()
        .filter(|(table, _)| table.contains("_a2"))
        .map(|(_, rows)| rows)
        .sum();
    assert_eq!(rows, 4, "one Edge row and three Path rows: {inserts:?}");
    assert_eq!(
        products(&db, "1_continue"),
        vec![(2, 3), (2, 6)],
        "three Edge rows and six Path rows"
    );
    let _ = std::fs::remove_dir_all(&directory);
}

#[test]
pub fn a_declared_relation_names_its_own_table() {
    let directory = scratch("names");
    let source = directory.join("1_continue.dl7");
    let program = directory.join("1_continue.json");
    let db = directory.join("store.db");

    std::fs::copy(fixture("1_continue.dl7"), &source).unwrap();
    transport(&source, &program);
    let first = evaluate(&program, &db, &source);
    assert_eq!(first.closure["diagnostics"], json!([]));

    let tables = sqlite_master(&db);
    for declared in ["1_continue.Edge_a2", "1_continue.Path_a2"] {
        assert!(
            tables.contains(&declared.to_string()),
            "{declared} missing from sqlite_master: {tables:?}"
        );
    }
    assert!(
        !tables.iter().any(|t| {
            t.strip_prefix("1_continue.rel")
                .is_some_and(|rest| rest.starts_with(|c: char| c.is_ascii_digit()))
        }),
        "a declared relation kept an arena-id table: {tables:?}"
    );

    std::fs::copy(fixture("1_continue_more.dl7"), &source).unwrap();
    transport(&source, &program);
    let second = evaluate(&program, &db, &source);
    assert_eq!(
        second.closure["closure"],
        expected("1_continue_more")["closure"],
        "continued closure"
    );
    let rows: u64 = inserts(&second)
        .iter()
        .filter(|(table, _)| table.ends_with("_a2"))
        .map(|(_, rows)| rows)
        .sum();
    assert_eq!(rows, 4, "the second run continued into the named tables");
    assert_eq!(sqlite_master(&db), tables, "a second table set was minted");
    let _ = std::fs::remove_dir_all(&directory);
}

// From tests/_18_sqlite_emit.rs
// `dl8 emit sqlite` against the closure, through the real binary and the real
// `sqlite_ivm` extension. Per fixture: `dl8 eval --db` creates the store, every
// emitted view is created over it, each view must equal its relation's closure,
// then one seed row is deleted through SQL and each view must equal the closure
// of the program without that seed. A fixture's emit diagnostics must equal
// `fixtures/sqlite_emit/expected_diagnostics.json` (absent key: none).
//
// The closure is `dl8 eval`'s, except for a program with a fold head: that
// closure comes from `dl8 compile` to preserve fixture source locations. Its
// reduced program is the source without the deleted seed's line.
//
// The extension is `SQLITE_IVM_LIB`, else the isolated release build in
// `sqlite_ivm/target/extension/release` (the sibling-checkout symlink), built with
// `cargo build --release --features extension --manifest-path sqlite_ivm/Cargo.toml`.

fn manifest() -> &'static Path {
    Path::new(env!("CARGO_MANIFEST_DIR"))
}

fn s18sqliteemit_normalize(value: &Value, path: &str) -> Value {
    match value {
        Value::String(s) => Value::String(s.replace(path, "<fixture>")),
        Value::Array(items) => Value::Array(
            items
                .iter()
                .map(|v| s18sqliteemit_normalize(v, path))
                .collect(),
        ),
        Value::Object(map) => Value::Object(
            map.iter()
                .map(|(k, v)| (k.clone(), s18sqliteemit_normalize(v, path)))
                .collect(),
        ),
        other => other.clone(),
    }
}

fn run(args: &[&str], paths: &[&Path]) -> Result<(Option<i32>, Value), String> {
    let output = Command::new(env!("CARGO_BIN_EXE_dl8"))
        .args(args)
        .args(paths)
        .output()
        .map_err(|e| format!("{args:?}: {e}"))?;
    let value = serde_json::from_slice(&output.stdout).map_err(|e| {
        format!(
            "{args:?}: no JSON ({e}); stderr: {}",
            String::from_utf8_lossy(&output.stderr)
        )
    })?;
    Ok((output.status.code(), value))
}

/// Rows per relation name, each row its argument list, sorted.
type Rows = BTreeMap<String, Vec<Value>>;

fn sorted(mut rows: Vec<Value>) -> Vec<Value> {
    rows.sort_by_key(|row| row.to_string());
    rows.dedup();
    rows
}

fn closure_rows(names: &Value, relations: &[String], calls: &[(Value, Value)]) -> Rows {
    relations
        .iter()
        .map(|name| {
            let rel = &names[name];
            let rows = calls
                .iter()
                .filter(|(found, _)| found == rel)
                .map(|(_, args)| args.clone())
                .collect();
            (name.clone(), sorted(rows))
        })
        .collect()
}

fn eval_closure(program: &Path, names: &Value, relations: &[String]) -> Result<Rows, String> {
    let (_, out) = run(&["eval"], &[program])?;
    let calls: Vec<(Value, Value)> = out["closure"]
        .as_array()
        .ok_or("eval: no closure")?
        .iter()
        .map(|row| (row["rel"].clone(), row["args"].clone()))
        .collect();
    Ok(closure_rows(names, relations, &calls))
}

fn compile_closure(compiled: &Value, names: &Value, relations: &[String]) -> Rows {
    let calls: Vec<(Value, Value)> = compiled["compiler_rows"]
        .as_array()
        .into_iter()
        .flatten()
        .filter(|row| row["f"] == "call")
        .map(|row| (row["args"][0].clone(), row["args"][1].clone()))
        .collect();
    closure_rows(names, relations, &calls)
}

fn has_fold_head(program: &Value) -> bool {
    program["rules"]
        .as_array()
        .into_iter()
        .flatten()
        .any(|rule| {
            rule["head"]["args"]
                .as_array()
                .into_iter()
                .flatten()
                .any(|arg| arg["f"] == "fold")
        })
}

/// Every view's rows. A `_term` column holds an arena id, an `_int` column the
/// integer of a `const` cell the view minted.
fn view_rows(
    db: &rusqlite::Connection,
    arena: &Universe,
    prefix: &str,
    views: &[Value],
) -> Result<Rows, String> {
    let mut out = Rows::new();
    for view in views {
        let relation = view["relation"].as_str().unwrap().to_string();
        let ddl = view["ddl"].as_str().unwrap();
        let table = ddl
            .strip_prefix("CREATE VIRTUAL TABLE ")
            .and_then(|rest| rest.split(" USING ").next())
            .ok_or("ddl shape")?;
        let mut statement = db
            .prepare(&format!("SELECT * FROM {table}"))
            .map_err(|e| format!("{prefix} {relation}: {e}"))?;
        let columns: Vec<String> = statement
            .column_names()
            .iter()
            .map(|c| c.to_string())
            .collect();
        let rows = statement
            .query_map((), |row| {
                let mut args = Vec::new();
                for (at, column) in columns.iter().enumerate() {
                    let cell: i64 = row.get(at)?;
                    if column.ends_with("_term") {
                        args.push(term_to_json(arena, TermId(cell as u32)));
                    } else if column.ends_with("_int") {
                        args.push(json!({"f": "const", "args": [cell]}));
                    }
                }
                Ok(Value::Array(args))
            })
            .map_err(|e| format!("{relation}: {e}"))?
            .collect::<Result<Vec<_>, _>>()
            .map_err(|e| format!("{relation}: {e}"))?;
        out.insert(relation, sorted(rows));
    }
    Ok(out)
}

fn compare(stage: &str, want: &Rows, got: &Rows) -> Vec<String> {
    want.iter()
        .filter(|(name, rows)| got.get(*name) != Some(*rows))
        .map(|(name, rows)| {
            format!(
                "{stage} {name}:\n    closure: {}\n    view:    {}",
                Value::Array(rows.clone()),
                got.get(name)
                    .map(|r| Value::Array(r.clone()))
                    .unwrap_or(Value::Null)
            )
        })
        .collect()
}

/// A seed's source line, `(Name 7 "text")`, for the fold path's reduced source.
fn seed_line(name: &str, args: &Value) -> String {
    let mut parts = vec![name.to_string()];
    for arg in args.as_array().unwrap() {
        let payload = &arg["args"][0];
        parts.push(match payload {
            Value::Object(m) if m.contains_key("s") => payload["s"].to_string(),
            Value::Object(m) if m.contains_key("a") => payload["a"].as_str().unwrap().to_string(),
            other => other.to_string(),
        });
    }
    format!("({})", parts.join(" "))
}

struct Receipt {
    fixture: String,
    relations: usize,
    recursive: bool,
    insert: Option<bool>,
    delete: Option<bool>,
    diagnostics: usize,
}

fn check(directory: &str, source: &Path, expected: &Value) -> Result<Receipt, Vec<String>> {
    let stem = source.file_stem().unwrap().to_string_lossy().to_string();
    let fixture = format!("{directory}/{stem}");
    let fail = |e: String| vec![format!("{fixture}: {e}")];
    let canonical = std::fs::canonicalize(source).unwrap();
    let path_text = canonical.display().to_string();
    let scratch = std::env::temp_dir().join(format!(
        "dl8-sqlite-emit-{}-{directory}-{stem}",
        std::process::id()
    ));
    let _ = std::fs::remove_dir_all(&scratch);
    std::fs::create_dir_all(scratch.join("reduced")).unwrap();
    let program_path = scratch.join(format!("{stem}.json"));
    let db_path = scratch.join("store.sqlite");

    let (_, compiled) = run(&["compile"], &[&canonical]).map_err(fail)?;
    let mut receipt = Receipt {
        fixture: fixture.clone(),
        relations: 0,
        recursive: false,
        insert: None,
        delete: None,
        diagnostics: 0,
    };
    if compiled["program"].is_null() {
        return Ok(receipt);
    }
    std::fs::write(&program_path, serde_json::to_vec(&compiled).unwrap()).unwrap();
    let program = &compiled["program"];
    let names = &program["names"];

    let (code, emitted) = run(&["emit", "sqlite"], &[&program_path]).map_err(fail)?;
    let diagnostics: Vec<Value> = emitted["diagnostics"]
        .as_array()
        .unwrap()
        .iter()
        .map(|d| s18sqliteemit_normalize(&d["payload"], &path_text))
        .collect();
    let want_diagnostics = expected.get(&fixture).cloned().unwrap_or(json!([]));
    let mut failures = Vec::new();
    if Value::Array(diagnostics.clone()) != want_diagnostics {
        failures.push(format!(
            "{fixture}: diagnostics\n    want: {want_diagnostics}\n    got:  {}",
            Value::Array(diagnostics.clone())
        ));
    }
    let want_code = if diagnostics.is_empty() { 0 } else { 1 };
    if code != Some(want_code) {
        failures.push(format!("{fixture}: emit exit {code:?}, want {want_code}"));
    }
    receipt.diagnostics = diagnostics.len();
    let views = emitted["views"].as_array().unwrap().clone();
    receipt.relations = views.len();
    receipt.recursive = views
        .iter()
        .any(|v| v["ddl"].as_str().unwrap().contains("WITH RECURSIVE"));
    if views.is_empty() {
        return if failures.is_empty() {
            Ok(receipt)
        } else {
            Err(failures)
        };
    }
    let relations: Vec<String> = views
        .iter()
        .map(|v| v["relation"].as_str().unwrap().to_string())
        .collect();

    let (code, _) = run(&["eval", "--db"], &[&db_path, &program_path]).map_err(fail)?;
    if code != Some(0) {
        failures.push(format!("{fixture}: eval --db exit {code:?}"));
        return Err(failures);
    }
    let folds = has_fold_head(program);
    let insert_closure = if folds {
        compile_closure(&compiled, names, &relations)
    } else {
        eval_closure(&program_path, names, &relations).map_err(fail)?
    };

    let db = open(&db_path).unwrap();
    for view in &views {
        let ddl = view["ddl"].as_str().unwrap();
        db.execute_batch(ddl)
            .map_err(|e| fail(format!("{e}\n    {ddl}")))?;
    }
    let mut arena = Universe::new();
    let mut store = SqliteRowStore::at(&db_path).unwrap();
    store.open(&stem).unwrap();
    store.load_arena(&mut arena).unwrap();

    let got = view_rows(&db, &arena, &stem, &views).map_err(fail)?;
    let insert = compare("insert", &insert_closure, &got);
    receipt.insert = Some(insert.is_empty());
    failures.extend(insert.into_iter().map(|e| format!("{fixture}: {e}")));

    let seeds = program["seeds"].as_array().unwrap();
    let Some(victim) = seeds
        .iter()
        .position(|seed| seed["rel"].to_string().contains(&path_text))
    else {
        failures.push(format!("{fixture}: no seed of the fixture's own relations"));
        return Err(failures);
    };
    let seed = &seeds[victim];
    let name = names
        .as_object()
        .unwrap()
        .iter()
        .find(|(_, rel)| **rel == seed["rel"])
        .map(|(name, _)| name.clone())
        .unwrap();
    let args = seed["args"].as_array().unwrap();
    let before = arena.terms.len();
    let cells: Vec<i64> = args
        .iter()
        .map(|arg| term_from_json(&mut arena, arg).unwrap().0 as i64)
        .collect();
    assert_eq!(
        arena.terms.len(),
        before,
        "{fixture}: seed cell missing from the arena"
    );
    let table = format!("\"{stem}.{name}_a{}\"", args.len());
    let filter = if cells.is_empty() {
        "1".to_string()
    } else {
        (0..cells.len())
            .map(|at| format!("\"c{at}_term\" = ?{}", at + 1))
            .collect::<Vec<_>>()
            .join(" AND ")
    };
    let deleted = db
        .execute(
            &format!("DELETE FROM {table} WHERE {filter}"),
            rusqlite::params_from_iter(cells.iter()),
        )
        .map_err(|e| fail(format!("delete: {e}")))?;
    if deleted != 1 {
        failures.push(format!("{fixture}: delete touched {deleted} rows"));
        return Err(failures);
    }

    let delete_closure = if folds {
        let text = std::fs::read_to_string(&canonical).unwrap();
        let line = seed_line(&name, &seed["args"]);
        let Some(at) = text.find(&line) else {
            failures.push(format!("{fixture}: no source line {line}"));
            return Err(failures);
        };
        let reduced_source = scratch.join("reduced").join(format!("{stem}.dl7"));
        std::fs::write(
            &reduced_source,
            format!("{}{}", &text[..at], &text[at + line.len()..]),
        )
        .unwrap();
        let reduced_source = std::fs::canonicalize(&reduced_source).unwrap();
        let (_, reduced) = run(&["compile"], &[&reduced_source]).map_err(fail)?;
        let reduced = s18sqliteemit_normalize(&reduced, &reduced_source.display().to_string());
        // The reduced compile's relation terms carry the reduced source's own
        // reader lines, so matching uses its names, never the original's.
        compile_closure(&reduced, &reduced["program"]["names"], &relations)
    } else {
        let mut reduced = compiled.clone();
        reduced["program"]["seeds"]
            .as_array_mut()
            .unwrap()
            .remove(victim);
        let reduced_path = scratch.join("reduced").join(format!("{stem}.json"));
        std::fs::write(&reduced_path, serde_json::to_vec(&reduced).unwrap()).unwrap();
        eval_closure(&reduced_path, names, &relations).map_err(fail)?
    };
    let got = view_rows(&db, &arena, &stem, &views).map_err(fail)?;
    let delete = compare("delete", &delete_closure, &got);
    receipt.delete = Some(delete.is_empty());
    failures.extend(delete.into_iter().map(|e| format!("{fixture}: {e}")));
    let _ = std::fs::remove_dir_all(&scratch);
    if failures.is_empty() {
        Ok(receipt)
    } else {
        Err(failures)
    }
}

fn mark(value: Option<bool>) -> &'static str {
    match value {
        Some(true) => "equal",
        Some(false) => "DIFFERS",
        None => "-",
    }
}

/// One `#[test]` per fixture, so each runs under its own timeout.
macro_rules! fixture_test {
    ($test:ident, $directory:literal, $stem:literal) => {
        #[test]
        fn $test() {
            let expected_path = manifest().join("fixtures/sqlite_emit/expected_diagnostics.json");
            let expected: Value =
                serde_json::from_str(&std::fs::read_to_string(&expected_path).unwrap()).unwrap();
            let source = manifest()
                .join("fixtures")
                .join($directory)
                .join(format!("{}.dl7", $stem));
            match check($directory, &source, &expected) {
                Ok(receipt) => println!(
                    "{}: views {} recursive {} diagnostics {} insert {} delete {}",
                    receipt.fixture,
                    receipt.relations,
                    if receipt.recursive { "yes" } else { "no" },
                    receipt.diagnostics,
                    mark(receipt.insert),
                    mark(receipt.delete)
                ),
                Err(found) => panic!("{}", found.join("\n")),
            }
        }
    };
}

fixture_test!(literals_0_float, "literals", "0_float");
fixture_test!(literals_1_bool, "literals", "1_bool");
fixture_test!(literals_2_int_add, "literals", "2_int_add");
fixture_test!(aggregates_0_sum, "aggregates", "0_sum");
fixture_test!(aggregates_1_min_max, "aggregates", "1_min_max");
fixture_test!(aggregates_2_grouped, "aggregates", "2_grouped");
fixture_test!(term_lt_0_mixed_kinds, "term_lt", "0_mixed_kinds");
fixture_test!(term_lt_1_top, "term_lt", "1_top");
fixture_test!(fold_0_kernel_step, "fold", "0_kernel_step");
fixture_test!(fold_1_program_step, "fold", "1_program_step");
fixture_test!(fold_2_order_matters, "fold", "2_order_matters");
fixture_test!(fold_3_step_no_row, "fold", "3_step_no_row");
fixture_test!(sqlite_emit_0_union_filter, "sqlite_emit", "0_union_filter");
fixture_test!(sqlite_emit_1_transitive, "sqlite_emit", "1_transitive");
fixture_test!(sqlite_emit_2_mutual, "sqlite_emit", "2_mutual");
fixture_test!(sqlite_emit_3_consumers, "sqlite_emit", "3_consumers");
fixture_test!(sqlite_emit_4_outside, "sqlite_emit", "4_outside");

// From tests/_25_sqlite_contract.rs
// The `open()` contract and the `sql` span under load: the eight rows of the
// eval-on-sqlite-ivm addendum. Every expectation is a COUNT, a ratio against
// a formula, or the one named wall clock (batched 10k under 50 ms). A file db
// under the test's temp dir, every connection through `open()`, every
// statement through `sql()`.

/// One closed span: its kind (`sql` or `phase`), its `name` field, the
/// recorded `rows` and `ms`, and the wall time between open and close.
#[derive(Clone)]
struct RecordedSpan {
    kind: String,
    name: Option<String>,
    rows: Option<i64>,
    ms: Option<f64>,
    started: Instant,
    elapsed_ms: Option<f64>,
}

impl tracing::field::Visit for RecordedSpan {
    fn record_str(&mut self, field: &tracing::field::Field, value: &str) {
        if field.name() == "name" {
            self.name = Some(value.to_string());
        }
    }

    fn record_i64(&mut self, field: &tracing::field::Field, value: i64) {
        if field.name() == "rows" {
            self.rows = Some(value);
        }
    }

    fn record_u64(&mut self, field: &tracing::field::Field, value: u64) {
        self.record_i64(field, value as i64);
    }

    fn record_f64(&mut self, field: &tracing::field::Field, value: f64) {
        if field.name() == "ms" {
            self.ms = Some(value);
        }
    }

    fn record_debug(&mut self, field: &tracing::field::Field, value: &dyn std::fmt::Debug) {
        if field.name() == "name" {
            self.name = Some(format!("{value:?}"));
        }
    }
}

/// The installed layer. Closed spans go down one channel.
#[derive(Clone)]
struct Collector(mpsc::Sender<RecordedSpan>);

impl<S> tracing_subscriber::Layer<S> for Collector
where
    S: tracing::Subscriber + for<'a> tracing_subscriber::registry::LookupSpan<'a>,
{
    fn on_new_span(
        &self,
        attrs: &tracing::span::Attributes<'_>,
        id: &tracing::span::Id,
        ctx: tracing_subscriber::layer::Context<'_, S>,
    ) {
        let Some(span) = ctx.span(id) else {
            return;
        };
        let mut recorded = RecordedSpan {
            kind: span.name().to_string(),
            name: None,
            rows: None,
            ms: None,
            started: Instant::now(),
            elapsed_ms: None,
        };
        attrs.record(&mut recorded);
        span.extensions_mut().insert(recorded);
    }

    fn on_record(
        &self,
        id: &tracing::span::Id,
        values: &tracing::span::Record<'_>,
        ctx: tracing_subscriber::layer::Context<'_, S>,
    ) {
        let Some(span) = ctx.span(id) else {
            return;
        };
        let mut extensions = span.extensions_mut();
        if let Some(recorded) = extensions.get_mut::<RecordedSpan>() {
            values.record(&mut *recorded);
        }
    }

    fn on_close(&self, id: tracing::span::Id, ctx: tracing_subscriber::layer::Context<'_, S>) {
        let Some(span) = ctx.span(&id) else {
            return;
        };
        let mut extensions = span.extensions_mut();
        if let Some(recorded) = extensions.get_mut::<RecordedSpan>() {
            recorded.elapsed_ms = Some(recorded.started.elapsed().as_secs_f64() * 1000.0);
            let _ = self.0.send(recorded.clone());
        }
    }
}

/// Runs `body` under the collector and returns every closed span.
fn traced<T>(body: impl FnOnce() -> T) -> Vec<RecordedSpan> {
    let (sender, receiver) = mpsc::channel();
    let guard =
        tracing::subscriber::set_default(tracing_subscriber::registry().with(Collector(sender)));
    let _ = body();
    drop(guard);
    receiver.try_iter().collect()
}

fn span_ms(spans: &[RecordedSpan], kind: &str, name: &str) -> f64 {
    spans
        .iter()
        .filter(|span| span.kind == kind && span.name.as_deref() == Some(name))
        .map(|span| span.ms.unwrap_or(0.0))
        .sum()
}

fn span_count(spans: &[RecordedSpan], kind: &str, name: &str) -> usize {
    spans
        .iter()
        .filter(|span| span.kind == kind && span.name.as_deref() == Some(name))
        .count()
}

/// RSS peak of this process, bytes (`getrusage` reports KiB off macOS).
fn max_rss_bytes() -> i64 {
    #[repr(C)]
    struct Rusage {
        utime: [i64; 2],
        stime: [i64; 2],
        maxrss: i64,
        rest: [i64; 13],
    }
    extern "C" {
        fn getrusage(who: i32, usage: *mut Rusage) -> i32;
    }
    let mut usage = std::mem::MaybeUninit::<Rusage>::zeroed();
    let code = unsafe { getrusage(0, usage.as_mut_ptr()) };
    assert_eq!(code, 0, "getrusage failed");
    let usage = unsafe { usage.assume_init() };
    if cfg!(target_os = "macos") {
        usage.maxrss
    } else {
        usage.maxrss * 1024
    }
}

fn directory(test: &str) -> PathBuf {
    let directory =
        std::env::temp_dir().join(format!("dl8-sqlite-contract-{}-{test}", std::process::id()));
    let _ = std::fs::remove_dir_all(&directory);
    std::fs::create_dir_all(&directory).unwrap();
    directory
}

/// The tests measure RSS peaks, so they never overlap each other.
fn lane() -> std::sync::MutexGuard<'static, ()> {
    static LANE: std::sync::Mutex<()> = std::sync::Mutex::new(());
    LANE.lock().unwrap_or_else(|poisoned| poisoned.into_inner())
}

fn declare(connection: &Connection, ddl: &str) {
    sql(connection, "declare", |connection| {
        connection.execute_batch(ddl).map(|()| ((), 0))
    })
    .unwrap();
}

fn begin(connection: &Connection) {
    sql(connection, "begin", |connection| {
        connection
            .execute_batch("BEGIN IMMEDIATE")
            .map(|()| ((), 0))
    })
    .unwrap();
}

fn commit(connection: &Connection) {
    sql(connection, "commit", |connection| {
        connection.execute_batch("COMMIT").map(|()| ((), 0))
    })
    .unwrap();
}

/// One multi-row INSERT, one `sql` span with its row count.
fn insert_values(connection: &Connection, table: &str, values: &[i64], name: &'static str) {
    sql(connection, name, |connection| {
        let tuples = vec!["(?)"; values.len()].join(",");
        let inserted = connection.execute(
            &format!("INSERT INTO \"{table}\" (\"n\") VALUES {tuples}"),
            rusqlite::params_from_iter(values.iter()),
        )?;
        Ok((inserted, inserted))
    })
    .unwrap();
}

/// One multi-row INSERT of `(left, right)` pairs, one `sql` span with its row
/// count.
fn insert_pairs(connection: &Connection, table: &str, columns: &str, pairs: &[(i64, i64)]) {
    sql(connection, "insert_rows", |connection| {
        let tuples = vec!["(?, ?)"; pairs.len()].join(",");
        let mut values = Vec::with_capacity(pairs.len() * 2);
        for (left, right) in pairs {
            values.push(*left);
            values.push(*right);
        }
        let inserted = connection.execute(
            &format!("INSERT INTO \"{table}\" ({columns}) VALUES {tuples}"),
            rusqlite::params_from_iter(values.iter()),
        )?;
        Ok((inserted, inserted))
    })
    .unwrap();
}

#[test]
fn per_statement_insert_costs_twenty_times_one_batched_insert() {
    let _lane = lane();
    let rows = 10_000i64;
    let directory = directory("n_plus_one");

    let statement_spans = traced(|| {
        let connection = open(&directory.join("per_statement.sqlite")).unwrap();
        declare(&connection, "CREATE TABLE \"t\" (\"n\" INTEGER NOT NULL)");
        for n in 0..rows {
            insert_values(&connection, "t", &[n], "insert_row");
        }
    });

    let batched_spans = traced(|| {
        let connection = open(&directory.join("batched.sqlite")).unwrap();
        declare(&connection, "CREATE TABLE \"t\" (\"n\" INTEGER NOT NULL)");
        begin(&connection);
        let values: Vec<i64> = (0..rows).collect();
        insert_values(&connection, "t", &values, "insert_rows");
        commit(&connection);
    });

    let per_statement_ms = span_ms(&statement_spans, "sql", "insert_row");
    let batched_ms = span_ms(&batched_spans, "sql", "insert_rows");
    println!(
        "per-statement {per_statement_ms:.1} ms over 10000 spans, batched {batched_ms:.1} ms over 1 span"
    );

    assert_eq!(span_count(&statement_spans, "sql", "insert_row"), 10_000);
    assert_eq!(span_count(&batched_spans, "sql", "insert_rows"), 1);
    assert!(
        batched_ms < 50.0,
        "batched insert {batched_ms:.1} ms, want under 50 ms"
    );
    assert!(
        per_statement_ms >= 20.0 * batched_ms,
        "per-statement {per_statement_ms:.1} ms, want >= 20x the batched {batched_ms:.1} ms"
    );
    let _ = std::fs::remove_dir_all(&directory);
}

#[test]
fn autocommit_span_count_matches_the_statement_count() {
    let _lane = lane();
    let rows = 10_000i64;
    let directory = directory("autocommit");
    let inserts = |connection: &Connection| {
        for n in 0..rows {
            insert_values(connection, "t", &[n], "insert_row");
        }
    };

    let autocommit_spans = traced(|| {
        let connection = open(&directory.join("autocommit.sqlite")).unwrap();
        declare(&connection, "CREATE TABLE \"t\" (\"n\" INTEGER NOT NULL)");
        inserts(&connection);
    });

    let transaction_spans = traced(|| {
        let connection = open(&directory.join("transaction.sqlite")).unwrap();
        declare(&connection, "CREATE TABLE \"t\" (\"n\" INTEGER NOT NULL)");
        begin(&connection);
        inserts(&connection);
        commit(&connection);
    });

    let autocommit_ms = span_ms(&autocommit_spans, "sql", "insert_row");
    let transaction_ms = span_ms(&transaction_spans, "sql", "insert_row");
    let ratio = autocommit_ms / transaction_ms.max(0.001);
    println!(
        "autocommit {autocommit_ms:.1} ms, one transaction {transaction_ms:.1} ms, ratio {ratio:.1}x (informational only)"
    );

    assert_eq!(
        span_count(&autocommit_spans, "sql", "insert_row"),
        rows as usize,
        "autocommit span count must equal the statement count"
    );
    assert_eq!(
        span_count(&transaction_spans, "sql", "insert_row"),
        rows as usize,
        "transaction span count must equal the statement count"
    );
    let _ = std::fs::remove_dir_all(&directory);
}

/// The `EXPLAIN QUERY PLAN` detail lines of one probe, indexed then not.
fn probe_plans(connection: &Connection, value: i64) -> (Vec<String>, Vec<String>) {
    let explain = |select: &str| {
        sql(connection, "explain", |connection| {
            let mut statement = connection.prepare(&format!("EXPLAIN QUERY PLAN {select}"))?;
            let mut cursor = statement.query([value])?;
            let mut detail = Vec::new();
            while let Some(row) = cursor.next()? {
                detail.push(row.get::<_, String>(3)?);
            }
            let count = detail.len();
            Ok((detail, count))
        })
        .unwrap()
    };
    let indexed = explain("SELECT \"n\" FROM \"lookup\" WHERE \"n\" = ?");
    let unindexed = explain("SELECT \"n\" FROM \"lookup\" WHERE \"payload\" = ?");
    (indexed, unindexed)
}

/// One probe through a fresh statement, so the VM step counter starts at
/// zero. The measured value carried on the span is the step count, never
/// the clock.
fn probe_once(connection: &Connection, name: &'static str, select: &str, value: i64) -> i64 {
    let mut steps = 0usize;
    sql(connection, name, |connection| {
        let mut statement = connection.prepare(select)?;
        let mut found = 0i64;
        {
            let mut cursor = statement.query([value])?;
            while let Some(row) = cursor.next()? {
                found = row.get(0)?;
            }
        }
        steps = statement.get_status(rusqlite::StatementStatus::VmStep) as usize;
        Ok((found, steps))
    })
    .unwrap();
    steps as i64
}

#[test]
fn unindexed_lookup_vm_steps_scale_five_times_at_hundred_k() {
    let _lane = lane();
    let directory = directory("lookup");
    let probes = 1_000i64;

    let probe_steps = |rows: i64| -> (i64, i64) {
        let mut unindexed_steps = 0i64;
        let mut indexed_steps = 0i64;
        traced(|| {
            let connection = open(&directory.join("lookup.sqlite")).unwrap();
            declare(
                &connection,
                "CREATE TABLE IF NOT EXISTS \"lookup\" \
                 (\"n\" INTEGER NOT NULL, \"payload\" INTEGER NOT NULL)",
            );
            let mut filled = sql(&connection, "count_rows", |connection| {
                let rows: i64 =
                    connection
                        .query_row("SELECT count(*) FROM \"lookup\"", (), |row| row.get(0))?;
                Ok((rows, 1))
            })
            .unwrap();
            if filled == 0 {
                declare(
                    &connection,
                    "CREATE INDEX \"lookup_n\" ON \"lookup\" (\"n\")",
                );
            }
            while filled < rows {
                let chunk: Vec<i64> = (filled..rows.min(filled + 15_000)).collect();
                begin(&connection);
                let pairs: Vec<(i64, i64)> = chunk.iter().map(|n| (*n, *n)).collect();
                insert_pairs(&connection, "lookup", "\"n\", \"payload\"", &pairs);
                commit(&connection);
                filled += chunk.len() as i64;
            }
            for probe in 0..probes {
                let value = probe * rows / probes;
                indexed_steps += probe_once(
                    &connection,
                    "probe_indexed",
                    "SELECT \"n\" FROM \"lookup\" WHERE \"n\" = ?",
                    value,
                );
                unindexed_steps += probe_once(
                    &connection,
                    "probe_unindexed",
                    "SELECT \"n\" FROM \"lookup\" WHERE \"payload\" = ?",
                    value,
                );
            }
        });
        (unindexed_steps, indexed_steps)
    };

    let (unindexed_10k, indexed_10k) = probe_steps(10_000);
    let (unindexed_100k, indexed_100k) = probe_steps(100_000);
    println!(
        "vm steps: unindexed 10k {unindexed_10k}, 100k {unindexed_100k}; indexed 10k {indexed_10k}, 100k {indexed_100k}"
    );
    // The plans are asserted, never timed: SCAN against the bare column,
    // SEARCH against the index.
    let connection = open(&directory.join("lookup.sqlite")).unwrap();
    let (indexed, unindexed) = probe_plans(&connection, 5_000);
    let joined = |plans: &[String]| plans.join(" | ");
    assert!(
        indexed.iter().any(|plan| plan.contains("SEARCH")),
        "indexed plan must SEARCH: {}",
        joined(&indexed)
    );
    assert!(
        unindexed.iter().any(|plan| plan.contains("SCAN")),
        "unindexed plan must SCAN: {}",
        joined(&unindexed)
    );
    assert!(
        unindexed_100k >= 5 * unindexed_10k,
        "unindexed vm steps at 100k {unindexed_100k}, want >= 5x the 10k count {unindexed_10k}"
    );
    assert!(
        indexed_100k <= 2 * indexed_10k,
        "indexed vm steps at 100k {indexed_100k}, want <= 2x the 10k count {indexed_10k}"
    );
}

#[test]
fn join_uses_the_index_on_the_inner_side() {
    let _lane = lane();
    let rows = 10_000i64;
    let directory = directory("join");
    let mut without = Vec::new();
    let mut with = Vec::new();
    traced(|| {
        let connection = open(&directory.join("join.sqlite")).unwrap();
        declare(
            &connection,
            "CREATE TABLE \"j_a\" (\"n\" INTEGER NOT NULL, \"b_id\" INTEGER NOT NULL);
             CREATE TABLE \"j_b\" (\"n\" INTEGER NOT NULL, \"tag\" INTEGER NOT NULL)",
        );
        begin(&connection);
        let a_pairs: Vec<(i64, i64)> = (0..rows).map(|n| (n, n)).collect();
        insert_pairs(&connection, "j_a", "\"n\", \"b_id\"", &a_pairs);
        insert_pairs(&connection, "j_b", "\"n\", \"tag\"", &a_pairs);
        commit(&connection);
        let join = "SELECT count(*) FROM \"j_a\" JOIN \"j_b\" \
                    ON \"j_a\".\"b_id\" = \"j_b\".\"n\" WHERE \"j_b\".\"tag\" < 100";
        let explain = |label: &'static str| {
            sql(&connection, label, |connection| {
                let mut prepared = connection.prepare(&format!("EXPLAIN QUERY PLAN {join}"))?;
                let mut cursor = prepared.query(())?;
                let mut detail = Vec::new();
                while let Some(row) = cursor.next()? {
                    detail.push(row.get::<_, String>(3)?);
                }
                let count = detail.len();
                Ok((detail, count))
            })
            .unwrap()
        };
        without = explain("explain_join_unindexed");
        declare(&connection, "CREATE INDEX \"j_b_n\" ON \"j_b\" (\"n\")");
        with = explain("explain_join_indexed");
    });
    let joined = |plans: &[String]| plans.join(" | ");
    assert!(
        without.iter().any(|plan| plan.contains("SCAN")),
        "unindexed join must SCAN somewhere: {}",
        joined(&without)
    );
    assert!(
        with.iter()
            .any(|plan| plan.contains("SEARCH j_b USING INDEX")),
        "indexed join must SEARCH j_b: {}",
        joined(&with)
    );
    assert!(
        !with.iter().any(|plan| plan.contains("SCAN j_b")),
        "indexed join must not SCAN the inner side: {}",
        joined(&with)
    );
    let _ = std::fs::remove_dir_all(&directory);
}

#[test]
fn contract_pragmas_read_back_from_a_file_db() {
    let _lane = lane();
    let directory = directory("pragmas");
    let connection = open(&directory.join("contract.sqlite")).unwrap();
    let read = |pragma: &str| -> String {
        let value: rusqlite::types::Value = connection
            .query_row(&format!("PRAGMA {pragma}"), (), |row| row.get(0))
            .unwrap();
        match value {
            rusqlite::types::Value::Integer(n) => n.to_string(),
            rusqlite::types::Value::Text(text) => text,
            other => panic!("unexpected value for {pragma}: {other:?}"),
        }
    };
    assert_eq!(read("page_size"), "65536");
    assert_eq!(read("journal_mode"), "wal");
    assert_eq!(read("synchronous"), "1");
    assert_eq!(read("mmap_size"), "1073741824");
    assert_eq!(read("cache_size"), "-262144");
    assert_eq!(read("temp_store"), "2");
    assert_eq!(read("recursive_triggers"), "1");
    assert_eq!(read("trusted_schema"), "1");

    let memory = open(Path::new(":memory:")).unwrap();
    let mode: String = memory
        .query_row("PRAGMA journal_mode", (), |row| row.get(0))
        .unwrap();
    assert_eq!(mode, "memory", "an in-memory db keeps its own journal mode");
    let _ = std::fs::remove_dir_all(&directory);
}

#[test]
fn cache_size_is_the_memory_ceiling() {
    let _lane = lane();
    let megabyte = 1024.0 * 1024.0;
    let payload = "x".repeat(16 * 1024);
    let rows = 4_096usize;
    let directory = directory("cache");

    let run = |file: &str, cache_kib: i64, before_rss: i64| -> (f64, i64) {
        let spans = traced(|| {
            let connection = open(&directory.join(file)).unwrap();
            sql(&connection, "cache_size", |connection| {
                connection
                    .execute_batch(&format!(
                        "PRAGMA mmap_size = 0; PRAGMA cache_size = {cache_kib};"
                    ))
                    .map(|()| ((), 0))
            })
            .unwrap();
            declare(
                &connection,
                "CREATE TABLE \"blob\" (\"n\" INTEGER NOT NULL, \"payload\" TEXT NOT NULL)",
            );
            begin(&connection);
            for chunk in 0..(rows / 32) {
                sql(&connection, "insert_blob_rows", |connection| {
                    let tuples = vec!["(?, ?)"; 32].join(",");
                    let mut values: Vec<rusqlite::types::Value> = Vec::with_capacity(64);
                    for n in (chunk * 32) as i64..((chunk + 1) * 32) as i64 {
                        values.push(rusqlite::types::Value::Integer(n));
                        values.push(rusqlite::types::Value::Text(payload.clone()));
                    }
                    let written = connection.execute(
                        &format!("INSERT INTO \"blob\" (\"n\", \"payload\") VALUES {tuples}"),
                        rusqlite::params_from_iter(values.iter()),
                    )?;
                    Ok((written, written))
                })
                .unwrap();
            }
            commit(&connection);
        });
        let ms = span_ms(&spans, "sql", "insert_blob_rows");
        let peak = max_rss_bytes();
        (ms, peak - before_rss)
    };

    let before = max_rss_bytes();
    let (ms_8mib, rss_8mib) = run("cache8.sqlite", -8192, before);
    let after_8 = max_rss_bytes();
    let (ms_256mib, rss_256mib) = run("cache256.sqlite", -262144, after_8);

    println!(
        "8 MiB cache: {ms_8mib:.1} ms, rss {:+.1} MiB; 256 MiB cache: {ms_256mib:.1} ms, rss {:+.1} MiB",
        rss_8mib as f64 / megabyte,
        rss_256mib as f64 / megabyte
    );

    assert!(
        rss_8mib as f64 <= 8.0 * megabyte + 32.0 * megabyte,
        "8 MiB cache run grew RSS {:.1} MiB, want under 40 MiB",
        rss_8mib as f64 / megabyte
    );
    assert!(
        rss_256mib as f64 <= 256.0 * megabyte + 32.0 * megabyte,
        "256 MiB cache run grew RSS {:.1} MiB, want under 288 MiB",
        rss_256mib as f64 / megabyte
    );
    assert!(
        ms_256mib <= ms_8mib,
        "256 MiB cache {ms_256mib:.1} ms, want no slower than the 8 MiB cache {ms_8mib:.1} ms"
    );
    let _ = std::fs::remove_dir_all(&directory);
}

#[test]
fn wal_checkpoint_bounds_the_wal() {
    let _lane = lane();
    let directory = directory("wal");
    let file = directory.join("wal.sqlite");
    let mut wal_before = 0u64;
    let spans = traced(|| {
        let connection = open(&file).unwrap();
        declare(&connection, "CREATE TABLE \"t\" (\"n\" INTEGER NOT NULL)");
        begin(&connection);
        let mut filled = 0i64;
        while filled < 50_000 {
            let chunk: Vec<i64> = (filled..50_000.min(filled + 25_000)).collect();
            insert_values(&connection, "t", &chunk, "insert_rows");
            filled += chunk.len() as i64;
        }
        commit(&connection);
        wal_before = std::fs::metadata(directory.join("wal.sqlite-wal"))
            .map(|meta| meta.len())
            .unwrap_or(0);
        sql(&connection, "wal_checkpoint", |connection| {
            let mut statement = connection.prepare("PRAGMA wal_checkpoint(TRUNCATE)")?;
            let mut cursor = statement.query(())?;
            let mut row = Vec::new();
            while let Some(line) = cursor.next()? {
                row.push(line.get::<_, i64>(0)?);
            }
            Ok((row, 1))
        })
        .unwrap();
    });
    assert!(
        wal_before > 0,
        "the wal must hold pages before the checkpoint truncates it"
    );
    assert_eq!(
        span_count(&spans, "sql", "wal_checkpoint"),
        1,
        "the checkpoint runs inside one sql span"
    );
    let wal_after = std::fs::metadata(directory.join("wal.sqlite-wal"))
        .map(|meta| meta.len())
        .unwrap_or(0);
    assert_eq!(wal_after, 0, "wal_checkpoint(TRUNCATE) must empty the wal");
    let _ = std::fs::remove_dir_all(&directory);
}

#[test]
fn rust_time_is_separable_from_sql_time() {
    let _lane = lane();
    let directory = directory("rust_ms");
    let connection = open(&directory.join("phases.sqlite")).unwrap();
    let spans = traced(|| {
        let phase = tracing::info_span!("phase", name = "contract");
        let _entered = phase.enter();
        declare(&connection, "CREATE TABLE \"t\" (\"n\" INTEGER NOT NULL)");
        let values: Vec<i64> = (0..1_000).collect();
        insert_values(&connection, "t", &values, "insert_rows");
        sql(&connection, "count_rows", |connection| {
            let rows: i64 =
                connection.query_row("SELECT count(*) FROM \"t\"", (), |row| row.get(0))?;
            Ok((rows, 1))
        })
        .unwrap();
    });

    let phase: Vec<&RecordedSpan> = spans.iter().filter(|span| span.kind == "phase").collect();
    assert_eq!(phase.len(), 1, "one phase span");
    let phase_ms = phase[0].elapsed_ms.unwrap();
    let sql_total: f64 = spans
        .iter()
        .filter(|span| span.kind == "sql")
        .map(|span| span.ms.unwrap_or(0.0))
        .sum();
    let rust_ms = phase_ms - sql_total;
    println!("phase {phase_ms:.1} ms, sql {sql_total:.1} ms, rust {rust_ms:.1} ms");
    assert!(sql_total > 0.0, "the sql spans must carry their ms");
    assert!(
        rust_ms >= 0.0,
        "rust ms {rust_ms:.1} below zero: sql spans {sql_total:.1} exceed the phase {phase_ms:.1}"
    );
    let _ = std::fs::remove_dir_all(&directory);
}

/// `main.unit` references in the sqlite engine's comptime view for the
/// prelude. SQLite expands a CTE at every reference and stops a statement at
/// 65535 references to one table, so identical bodyless rules share one
/// select and no product's CTE reads `main.unit` twice.
const PRELUDE_UNIT_REFERENCES: usize = 15;

#[test]
fn sqlite_view_reads_unit_once_per_product() {
    let root = Path::new(env!("CARGO_MANIFEST_DIR"));
    let compiled = Command::new(env!("CARGO_BIN_EXE_dl8"))
        .arg("compile")
        .arg(root.join("fixtures/generic/2_variable_goal.dl7"))
        .env("DL8_ENGINE", "sqlite")
        .env("RUST_LOG", "dl8::eval=debug")
        .output()
        .unwrap();
    assert!(compiled.status.success());
    let log = String::from_utf8(compiled.stderr).unwrap();
    let views: Vec<&str> = log.lines().filter(|line| line.contains("declare_view sql")).collect();
    assert!(!views.is_empty());
    let mut most = 0;
    for view in views {
        let total = view.matches("main.unit").count();
        most = most.max(total);
        for body in view.split(" AS (") {
            assert!(body.matches("main.unit").count() <= 1, "{body}");
        }
    }
    assert!(most <= PRELUDE_UNIT_REFERENCES, "{most}");
}

/// Objects the ir:sqlite engine creates for the prelude's comptime program.
/// A rule that reads a generic back and builds another one is not demanded,
/// so no reader of `:` gets an adorned copy of every `:` rule.
const PRELUDE_COMPTIME_SQLITE_OBJECTS: usize = 2895;

#[test]
fn ir_sqlite_comptime_objects_for_the_prelude() {
    let root = Path::new(env!("CARGO_MANIFEST_DIR"));
    let compiled = Command::new(env!("CARGO_BIN_EXE_dl8"))
        .arg("compile")
        .arg(root.join("fixtures/generic/2_variable_goal.dl7"))
        .env("DL8_ENGINE", "ir:sqlite")
        .env("RUST_LOG", "info")
        .output()
        .unwrap();
    assert!(compiled.status.success());
    let log = String::from_utf8(compiled.stderr).unwrap();
    let created: Vec<usize> = log
        .lines()
        .filter(|line| line.contains("name=\"comptime\"") && line.contains("sqlite_create_count="))
        .filter_map(|line| line.rsplit("sqlite_create_count=").next()?.trim().parse().ok())
        .collect();
    assert_eq!(created.len(), 1, "{log}");
    assert!(created[0] <= PRELUDE_COMPTIME_SQLITE_OBJECTS, "{}", created[0]);
}
