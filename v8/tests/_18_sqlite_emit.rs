//! `dl8 emit sqlite` against the closure, through the real binary and the real
//! `sqlite_ivm` extension. Per fixture: `dl8 eval --db` creates the store, every
//! emitted view is created over it, each view must equal its relation's closure,
//! then one seed row is deleted through SQL and each view must equal the closure
//! of the program without that seed. A fixture's emit diagnostics must equal
//! `fixtures/sqlite_emit/expected_diagnostics.json` (absent key: none).
//!
//! The closure is `dl8 eval`'s, except for a program with a fold head: the eval
//! JSON transport carries no `fold` key (`_15_fold.rs` header), so that closure
//! is the one `dl8 compile` evaluates, and its reduced program is the source
//! without the deleted seed's line.
//!
//! The extension is `SQLITE_IVM_LIB`, else `libsqlite_ivm` in the release dir
//! of `CARGO_TARGET_DIR` or `../sqlite_ivm/target`, built with
//! `cargo build --release --features extension --manifest-path sqlite_ivm/Cargo.toml`.

use dl8::_6_eval::json::{term_from_json, term_to_json};
use dl8::_6_eval::{TermId, Universe};
use dl8::_9_runtime::{IRowStore, SqliteRowStore};
use serde_json::{json, Value};
use std::collections::BTreeMap;
use std::path::{Path, PathBuf};
use std::process::Command;

fn manifest() -> &'static Path {
    Path::new(env!("CARGO_MANIFEST_DIR"))
}

fn extension() -> PathBuf {
    let file = if cfg!(target_os = "macos") {
        "libsqlite_ivm.dylib"
    } else {
        "libsqlite_ivm.so"
    };
    let mut candidates = Vec::new();
    if let Ok(path) = std::env::var("SQLITE_IVM_LIB") {
        candidates.push(PathBuf::from(path));
    }
    if let Ok(dir) = std::env::var("CARGO_TARGET_DIR") {
        candidates.push(Path::new(&dir).join("release").join(file));
    }
    candidates.push(manifest().join("../sqlite_ivm/target/release").join(file));
    candidates
        .iter()
        .find(|path| path.exists())
        .cloned()
        .unwrap_or_else(|| {
            panic!(
                "no sqlite_ivm extension at {candidates:?}; run `cargo build --release --features extension --manifest-path sqlite_ivm/Cargo.toml`"
            )
        })
}

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

fn check(
    directory: &str,
    source: &Path,
    expected: &Value,
    ivm: &Path,
) -> Result<Receipt, Vec<String>> {
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
        .map(|d| normalize(&d["payload"], &path_text))
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

    let db = rusqlite::Connection::open(&db_path).unwrap();
    // SAFETY: the library is this repo's own sqlite_ivm build, loaded once into a test connection.
    unsafe { db.load_extension(ivm, None::<&str>) }
        .map_err(|e| fail(format!("load {}: {e}", ivm.display())))?;
    db.execute_batch("PRAGMA recursive_triggers=ON; PRAGMA trusted_schema=ON;")
        .unwrap();
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
        let reduced = normalize(&reduced, &reduced_source.display().to_string());
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
            let ivm = extension();
            let expected_path = manifest().join("fixtures/sqlite_emit/expected_diagnostics.json");
            let expected: Value =
                serde_json::from_str(&std::fs::read_to_string(&expected_path).unwrap()).unwrap();
            let source = manifest()
                .join("fixtures")
                .join($directory)
                .join(format!("{}.dl7", $stem));
            match check($directory, &source, &expected, &ivm) {
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
