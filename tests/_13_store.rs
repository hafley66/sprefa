//! The closure at rest, through the real binary. Each `fixtures/store/*.dl7`
//! compiles to a checked runtime program, which `dl8 eval --db` evaluates into
//! one SQLite file; a second run against the same file loads the arena and the
//! rows back and derives only what the new seeds add. The fixture path is
//! replaced by `<fixture>` so the expected files carry no absolute path.
//!
//! `dl8 compile`'s whole stdout is the program file; `dl8 eval` reads its
//! `program` key, and the store names each table after the declared relation.

use dl8::_6_eval::{TermId, Universe};
use dl8::_9_runtime::{open, IRowStore, SqliteRowStore, Watermark};
use serde_json::{json, Value};
use std::path::{Path, PathBuf};
use std::process::Command;

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
    assert_eq!(
        second.closure, first.closure,
        "closure differs on the reloaded run"
    );
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
