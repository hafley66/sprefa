#![cfg(feature = "cli")]
#![allow(dead_code)]

#[path = "../src/bin/extract/0_sqlite.rs"]
mod sqlite;

use rusqlite::{types::Value as SqlValue, Connection};
use serde::Deserialize;
use serde_json::{json, Value};
use std::collections::BTreeSet;
use std::path::Path;
use std::process::Command;

fn run(args: &[&str]) -> std::process::Output {
    Command::new(env!("CARGO_BIN_EXE_extract"))
        .args(args)
        .env("DL_TRAIL", "0")
        .env("DL_TRACE", "0")
        .output()
        .unwrap()
}

const CATALOG: &str = include_str!(concat!(
    env!("CARGO_MANIFEST_DIR"),
    "/../../schema/generated/5_facts.json"
));

#[derive(Deserialize)]
struct Column {
    name: String,
    path: Vec<String>,
    kind: String,
    optional: bool,
    nullable: bool,
    literal: Option<String>,
    values: Option<Vec<String>>,
}

#[derive(Deserialize)]
struct Table {
    record: String,
    table: String,
    columns: Vec<Column>,
}

fn tables() -> Vec<Table> {
    serde_json::from_str(CATALOG).unwrap()
}
fn quote(s: &str) -> String {
    format!("\"{}\"", s.replace('"', "\"\""))
}

fn sql_values(value: &Value, columns: &[&Column]) -> Vec<SqlValue> {
    columns
        .iter()
        .map(|c| {
            let value = c.path.iter().try_fold(value, |v, k| v.get(k));
            match value {
                None => SqlValue::Null,
                Some(v) if c.kind == "json" => SqlValue::Text(v.to_string()),
                Some(Value::Null) => SqlValue::Null,
                Some(Value::String(s)) => SqlValue::Text(s.clone()),
                Some(Value::Bool(b)) => SqlValue::Integer(i64::from(*b)),
                Some(Value::Number(n)) => n
                    .as_i64()
                    .map_or_else(|| SqlValue::Text(n.to_string()), SqlValue::Integer),
                other => panic!("Unhandled {other:?}"),
            }
        })
        .collect()
}

fn assert_rows(db: &Connection, expected: &[Value]) {
    assert_rows_at(db, expected, 1, true);
}

fn assert_rows_at(db: &Connection, expected: &[Value], first_row: i64, exact: bool) {
    let catalog = tables();
    for (i, value) in expected.iter().enumerate() {
        let table = catalog
            .iter()
            .find(|t| value["record"] == t.record)
            .unwrap();
        let columns: Vec<_> = table
            .columns
            .iter()
            .filter(|c| !c.name.starts_with('_'))
            .collect();
        let sql = format!(
            "SELECT {} FROM {} WHERE _row=?",
            columns
                .iter()
                .map(|c| quote(&c.name))
                .collect::<Vec<_>>()
                .join(","),
            quote(&table.table)
        );
        let got = db
            .query_row(&sql, [first_row + i as i64], |r| {
                (0..columns.len())
                    .map(|col| r.get::<_, SqlValue>(col))
                    .collect::<rusqlite::Result<Vec<_>>>()
            })
            .unwrap_or_else(|e| panic!("{sql}: {e}"));
        let want = sql_values(value, &columns);
        assert_eq!(got, want, "row {}: {}", first_row + i as i64, value);
    }
    let count: i64 = catalog
        .iter()
        .map(|t| {
            db.query_row(
                &format!("SELECT count(*) FROM {}", quote(&t.table)),
                [],
                |r| r.get::<_, i64>(0),
            )
            .unwrap()
        })
        .sum();
    if exact {
        assert_eq!(count, expected.len() as i64);
    }
    assert_eq!(
        db.query_row("PRAGMA integrity_check", [], |r| r.get::<_, String>(0))
            .unwrap(),
        "ok"
    );
}

fn assert_uncoordinated_rows(db: &Connection, expected: &[Value]) {
    for table in tables() {
        let columns: Vec<_> = table
            .columns
            .iter()
            .filter(|column| !column.name.starts_with('_'))
            .collect();
        let sql = format!(
            "SELECT {} FROM {} WHERE _input_path IS NULL AND _content_id IS NULL ORDER BY _row",
            columns
                .iter()
                .map(|column| quote(&column.name))
                .collect::<Vec<_>>()
                .join(","),
            quote(&table.table),
        );
        let mut got = db
            .prepare(&sql)
            .unwrap()
            .query_map([], |row| {
                (0..columns.len())
                    .map(|column| row.get::<_, SqlValue>(column))
                    .collect::<rusqlite::Result<Vec<_>>>()
            })
            .unwrap()
            .collect::<rusqlite::Result<Vec<_>>>()
            .unwrap();
        let mut want: Vec<_> = expected
            .iter()
            .filter(|value| value["record"] == table.record)
            .map(|value| sql_values(value, &columns))
            .collect();
        got.sort_by_key(|row| format!("{row:?}"));
        want.sort_by_key(|row| format!("{row:?}"));
        assert_eq!(got, want, "{}", table.table);
    }
}

#[test]
fn every_flat_fact_variant_and_field_has_a_typespec_table() {
    let source = syn::parse_file(include_str!("../src/types.rs")).unwrap();
    let flat = source
        .items
        .iter()
        .find_map(|item| match item {
            syn::Item::Enum(e) if e.ident == "FlatFact" => Some(e),
            _ => None,
        })
        .unwrap();
    let catalog = tables();
    fn renamed(attrs: &[syn::Attribute]) -> Option<String> {
        let mut result = None;
        for attr in attrs.iter().filter(|a| a.path().is_ident("serde")) {
            attr.parse_nested_meta(|meta| {
                if meta.path.is_ident("rename") {
                    result = Some(meta.value()?.parse::<syn::LitStr>()?.value());
                } else if meta.input.peek(syn::Token![=]) {
                    let _: syn::Expr = meta.value()?.parse()?;
                }
                Ok(())
            })
            .unwrap();
        }
        result
    }
    let mut covered = BTreeSet::new();
    for variant in &flat.variants {
        let tag =
            renamed(&variant.attrs).unwrap_or_else(|| variant.ident.to_string().to_lowercase());
        let table = catalog
            .iter()
            .find(|t| t.record == tag)
            .unwrap_or_else(|| panic!("Missing TypeSpec table: {tag}"));
        covered.insert(tag.clone());
        if let syn::Fields::Named(fields) = &variant.fields {
            let rust: BTreeSet<_> = fields
                .named
                .iter()
                .map(|f| renamed(&f.attrs).unwrap_or_else(|| f.ident.as_ref().unwrap().to_string()))
                .collect();
            let tsp: BTreeSet<_> = table
                .columns
                .iter()
                .map(|c| c.path[0].clone())
                .filter(|p| p != "record" && !p.starts_with('_'))
                .collect();
            assert_eq!(tsp, rust, "Field drift: {tag}");
        }
    }
    let extras: BTreeSet<_> = catalog
        .iter()
        .map(|t| t.record.clone())
        .filter(|t| !covered.contains(t))
        .collect();
    assert_eq!(extras, BTreeSet::from(["capture".to_owned()]));
    let tsi = syn::parse_file(include_str!("../src/tsi/types.rs")).unwrap();
    for (file, enum_name, table_name, column_name, snake_case) in [
        (&source, "FamilyTag", "node", "family", false),
        (&tsi, "Mode", "run", "mode", false),
        (&tsi, "Method", "witness", "method", true),
    ] {
        let e = file
            .items
            .iter()
            .find_map(|item| match item {
                syn::Item::Enum(e) if e.ident == enum_name => Some(e),
                _ => None,
            })
            .unwrap();
        let names: Vec<String> = e
            .variants
            .iter()
            .map(|v| {
                let name = v.ident.to_string();
                let mut result = String::new();
                for (i, ch) in name.chars().enumerate() {
                    if snake_case && i > 0 && ch.is_ascii_uppercase() {
                        result.push('_');
                    }
                    result.push(ch.to_ascii_lowercase());
                }
                result
            })
            .collect();
        let column = catalog
            .iter()
            .find(|t| t.record == table_name)
            .unwrap()
            .columns
            .iter()
            .find(|c| c.name == column_name)
            .unwrap();
        assert_eq!(
            column.values.as_ref().unwrap(),
            &names,
            "Enum drift: {enum_name}"
        );
    }
}

#[test]
fn every_table_accepts_a_complete_row_and_flatfact_deserializes_it() {
    let scratch = tempfile::tempdir().unwrap();
    let path = scratch.path().join("all.db");
    let mut db = sqlite::Database::create(&path).unwrap();
    let mut rows = Vec::new();
    for table in tables() {
        let mut value = json!({});
        for column in &table.columns {
            if column.name.starts_with('_') {
                continue;
            }
            let sample = if let Some(literal) = &column.literal {
                json!(literal)
            } else if let Some(values) = &column.values {
                json!(values[0])
            } else if column.nullable {
                Value::Null
            } else {
                match column.kind.as_str() {
                    "string" => json!("sample'\"λ"),
                    "boolean" => json!(true),
                    "json" => json!([]),
                    _ => json!(1),
                }
            };
            let mut at = &mut value;
            for part in &column.path[..column.path.len() - 1] {
                at = at
                    .as_object_mut()
                    .unwrap()
                    .entry(part.clone())
                    .or_insert_with(|| json!({}));
            }
            at.as_object_mut()
                .unwrap()
                .insert(column.path.last().unwrap().clone(), sample);
        }
        if table.record != "capture" {
            serde_json::from_value::<sprefa_extract::FlatFact>(value.clone())
                .unwrap_or_else(|e| panic!("{}: {e}: {value}", table.record));
        }
        db.insert(value.clone())
            .unwrap_or_else(|e| panic!("{}: {e}: {value}", table.record));
        rows.push(value);
    }
    db.finish().unwrap();
    assert_rows(&Connection::open(path).unwrap(), &rows);
}

#[test]
fn source_families_cfg_witness_and_data_match_jsonl_column_for_column() {
    let scratch = tempfile::tempdir().unwrap();
    for (i, args) in [
        vec!["--file-fact", "tests/fixtures/ts/sample.ts"],
        vec!["--file-fact", "tests/fixtures/rust/sample.rs"],
        vec!["--file-fact", "tests/fixtures/go/sample.go"],
        vec!["--file-fact", "tests/fixtures/kotlin/sample.kt"],
        vec!["--file-fact", "tests/fixtures/prolog/0_sample.pl"],
        vec!["--file-fact", "tests/fixtures/markdown/0_sample.md"],
        vec!["--file-fact", "tests/fixtures/data/nested.json"],
        vec![
            "--file-fact",
            "--family",
            "cfg",
            "tests/fixtures/rust/sample.rs",
        ],
        vec!["--witness", "tests/fixtures/ts/sample.ts"],
    ]
    .into_iter()
    .enumerate()
    {
        let plain = run(&args);
        assert!(
            plain.status.success(),
            "{}",
            String::from_utf8_lossy(&plain.stderr)
        );
        let mut expected: Vec<Value> = String::from_utf8(plain.stdout)
            .unwrap()
            .lines()
            .map(|s| serde_json::from_str(s).unwrap())
            .collect();
        if args.contains(&"--witness") {
            let source = args.last().unwrap();
            expected.insert(
                0,
                serde_json::to_value(sprefa_extract::file_fact(
                    source,
                    &std::fs::read(source).unwrap(),
                ))
                .unwrap(),
            );
        }
        assert!(!expected.is_empty());
        let path = scratch.path().join(format!("{i}.db"));
        let mut sql_args = vec!["--sqlite", path.to_str().unwrap()];
        sql_args.extend(args);
        let exported = run(&sql_args);
        assert!(
            exported.status.success(),
            "{}",
            String::from_utf8_lossy(&exported.stderr)
        );
        let stdout = String::from_utf8(exported.stdout).unwrap();
        assert!(stdout.contains("'.schema'"), "{stdout}");
        assert_rows(&Connection::open(&path).unwrap(), &expected);
    }
}

#[test]
fn multiple_files_keep_their_path_and_content_coordinates() {
    let scratch = tempfile::tempdir().unwrap();
    let path = scratch.path().join("two.db");
    let files = [
        "tests/fixtures/ts/sample.ts",
        "tests/fixtures/rust/sample.rs",
    ];
    let output = run(&[
        "--sqlite",
        path.to_str().unwrap(),
        "--family",
        "call",
        files[0],
        files[1],
    ]);
    assert!(
        output.status.success(),
        "{}",
        String::from_utf8_lossy(&output.stderr)
    );
    let db = Connection::open(path).unwrap();
    let rows = db
        .prepare("SELECT path, digest, _input_path, _content_id FROM file ORDER BY _row")
        .unwrap()
        .query_map([], |r| {
            Ok((
                r.get::<_, String>(0)?,
                r.get::<_, String>(1)?,
                r.get::<_, String>(2)?,
                r.get::<_, String>(3)?,
            ))
        })
        .unwrap()
        .collect::<rusqlite::Result<Vec<_>>>()
        .unwrap();
    let expected: Vec<_> = files
        .iter()
        .map(|f| {
            let digest = sprefa_extract::content_id_of(&std::fs::read(f).unwrap()).to_string();
            (f.to_string(), digest.clone(), f.to_string(), digest)
        })
        .collect();
    assert_eq!(rows, expected);
}

#[test]
fn project_scip_dependency_and_pattern_modes_match_their_existing_jsonl() {
    let scratch = tempfile::tempdir().unwrap();
    let root = "tests/fixtures/scip_relationship";
    let index = "tests/fixtures/scip_relationship/fixture.scip";
    let source = "tests/fixtures/scip_relationship/animal.ts";
    for (i, args) in [
        vec![
            "fast",
            "tests/fixtures/ts/scip/alpha.ts",
            "tests/fixtures/ts/scip/gamma.ts",
        ],
        vec![
            "--resolve",
            "--family",
            "call,type,flow",
            "tests/fixtures/ts/scip/alpha.ts",
            "tests/fixtures/ts/scip/gamma.ts",
        ],
        vec![
            "--scip-facts",
            "--occurrence-text",
            "--scip-index",
            index,
            "--project-root",
            root,
            source,
        ],
        vec!["--family", "scip", "--scip-index", index, root],
        vec![
            "--scip-deps",
            "--scip-index",
            "tests/fixtures/scip_move/fixture.scip",
            "--project-root",
            "tests/fixtures/scip_move",
            "tests/fixtures/scip_move/src/app.ts",
        ],
        vec![
            "--deps",
            "--project-root",
            "tests/fixtures/ts/scip",
            "tests/fixtures/ts/scip/alpha.ts",
            "tests/fixtures/ts/scip/gamma.ts",
        ],
        vec![
            "--ast-pattern",
            "create=createApi($CONFIG)",
            "--ast-capture",
            "create=CONFIG",
            "tests/fixtures/ast_pattern/0_rtkq.ts",
        ],
    ]
    .into_iter()
    .enumerate()
    {
        let plain = run(&args);
        assert!(
            plain.status.success(),
            "{args:?}: {}",
            String::from_utf8_lossy(&plain.stderr)
        );
        let mut expected: Vec<Value> = String::from_utf8(plain.stdout)
            .unwrap()
            .lines()
            .map(|s| serde_json::from_str(s).unwrap())
            .collect();
        assert!(!expected.is_empty(), "Empty test input: {args:?}");
        let retains_raw = args.first() == Some(&"fast") || args.contains(&"--resolve");
        if args.contains(&"--ast-pattern") {
            assert!(expected.iter().any(|v| v["record"] == "capture"));
            let source = args.last().unwrap();
            expected.insert(
                0,
                serde_json::to_value(sprefa_extract::file_fact(
                    source,
                    &std::fs::read(source).unwrap(),
                ))
                .unwrap(),
            );
        }
        let path = scratch.path().join(format!("project-{i}.db"));
        let mut sql_args = args.clone();
        sql_args.extend(["--sqlite", path.to_str().unwrap()]);
        let exported = run(&sql_args);
        assert!(
            exported.status.success(),
            "{args:?}: {}",
            String::from_utf8_lossy(&exported.stderr)
        );
        let db = Connection::open(path).unwrap();
        if retains_raw {
            let total: i64 = tables()
                .iter()
                .map(|table| {
                    db.query_row(
                        &format!("SELECT count(*) FROM {}", quote(&table.table)),
                        [],
                        |row| row.get::<_, i64>(0),
                    )
                    .unwrap()
                })
                .sum();
            assert!(total > expected.len() as i64);
            let first_resolved = total - expected.len() as i64 + 1;
            assert_uncoordinated_rows(&db, &expected);
            let sourced: i64 = tables()
                .iter()
                .map(|table| {
                    db.query_row(
                        &format!(
                            "SELECT count(*) FROM {} WHERE _row < ? AND _input_path IS NOT NULL AND _content_id IS NOT NULL",
                            quote(&table.table)
                        ),
                        [first_resolved],
                        |row| row.get::<_, i64>(0),
                    )
                    .unwrap()
                })
                .sum();
            assert_eq!(sourced, first_resolved - 1);
            let inherited: i64 = tables()
                .iter()
                .map(|table| {
                    db.query_row(
                        &format!(
                            "SELECT count(*) FROM {} WHERE _row >= ? AND (_input_path IS NOT NULL OR _content_id IS NOT NULL)",
                            quote(&table.table)
                        ),
                        [first_resolved],
                        |row| row.get::<_, i64>(0),
                    )
                    .unwrap()
                })
                .sum();
            assert_eq!(inherited, 0);
        } else {
            assert_rows(&db, &expected);
        }
    }
}

#[test]
fn foreign_witness_stream_is_preserved_and_bad_ingest_is_atomic() {
    let scratch = tempfile::tempdir().unwrap();
    let stream = scratch.path().join("stream.jsonl");
    let plain = run(&["--witness", "tests/fixtures/ts/sample.ts"]);
    assert!(plain.status.success());
    std::fs::write(&stream, plain.stdout).unwrap();
    let ingested = run(&["--ingest", stream.to_str().unwrap()]);
    assert!(
        ingested.status.success(),
        "{}",
        String::from_utf8_lossy(&ingested.stderr)
    );
    let expected: Vec<Value> = String::from_utf8(ingested.stdout)
        .unwrap()
        .lines()
        .map(|s| serde_json::from_str(s).unwrap())
        .collect();
    let path = scratch.path().join("foreign.db");
    let exported = run(&[
        "--ingest",
        stream.to_str().unwrap(),
        "--sqlite",
        path.to_str().unwrap(),
    ]);
    assert!(
        exported.status.success(),
        "{}",
        String::from_utf8_lossy(&exported.stderr)
    );
    assert_rows(&Connection::open(&path).unwrap(), &expected);
    std::fs::write(&stream, "not JSON\n").unwrap();
    let bad_path = scratch.path().join("bad.db");
    let bad = run(&[
        "--ingest",
        stream.to_str().unwrap(),
        "--sqlite",
        bad_path.to_str().unwrap(),
    ]);
    assert!(!bad.status.success());
    assert!(!bad_path.exists());
    assert!(!std::fs::read_dir(scratch.path()).unwrap().any(|p| p
        .unwrap()
        .file_name()
        .to_string_lossy()
        .starts_with(".extract-sqlite-")));
}

#[test]
fn malformed_rows_failed_extraction_and_existing_paths_never_publish_or_overwrite() {
    let scratch = tempfile::tempdir().unwrap();
    let path = scratch.path().join("facts.db");
    for value in [
        json!({"record":"new_record"}),
        json!({"record":"protocol","version":1,"surprise":true}),
        json!({"record":"protocol","version":-1}),
        json!({"record":"protocol"}),
        json!({"record":"protocol","version":1,"_input_path":"forged"}),
        json!({"record":"run","run":0,"mode":"invented","tool":"x","version":"1","scope":[]}),
        json!({"record":"fact","fact":1,"relation":"x.y","args":[{"span":["digest",4]}]}),
        json!({"record":"fact","fact":1,"relation":"x.y","args":[{"id":-1}]}),
    ] {
        let mut db = sqlite::Database::create(&path).unwrap();
        assert!(db.insert(value).is_err());
        drop(db);
        assert!(!path.exists());
        assert_eq!(std::fs::read_dir(scratch.path()).unwrap().count(), 0);
    }
    let failed = run(&[
        "--sqlite",
        path.to_str().unwrap(),
        "--family",
        "nonsense",
        "tests/fixtures/ts/sample.ts",
    ]);
    assert!(!failed.status.success());
    assert!(!path.exists());
    assert_eq!(std::fs::read_dir(scratch.path()).unwrap().count(), 0);
    std::fs::write(&path, "preserve this").unwrap();
    let failed = run(&[
        "--sqlite",
        path.to_str().unwrap(),
        "tests/fixtures/ts/sample.ts",
    ]);
    assert!(!failed.status.success());
    assert_eq!(std::fs::read(&path).unwrap(), b"preserve this");
    assert!(sqlite::Database::create(Path::new(":memory:")).is_err());
}

#[test]
fn values_duplicates_unsigned_limits_and_publication_races_are_preserved() {
    let scratch = tempfile::tempdir().unwrap();
    let path = scratch.path().join("values.db");
    let rows = vec![
        json!({"record":"size_skip","path":"quotes'\"\nλ","bytes":u64::MAX,"limit":0,"reason":"over_max_bytes"}),
        json!({"record":"size_skip","path":"signed-max","bytes":i64::MAX as u64,"limit":i64::MAX as u64 + 1,"reason":"over_max_bytes"}),
        json!({"record":"scip_index","reused":false,"tool_name":"fixture","tool_version":"1","documents":0,"index_mtime_unix_ms":u64::MAX,"staleness":"fixture"}),
        json!({"record":"scip_index","reused":true,"tool_name":"fixture","tool_version":"1","documents":0,"index_mtime_unix_ms":null,"staleness":"fixture"}),
        json!({"record":"protocol","version":1}),
        json!({"record":"protocol","version":1}),
        json!({"record":"data_doc","family":"data","ordinal":0,"span":{"start":0,"end":4},"format":"json","doc":null}),
        json!({"record":"fact","fact":1,"relation":"x.y","args":[{"id":u32::MAX},{"span":["digest",0,u32::MAX]},{"int":i64::MIN},{"text":"λ"},{"atom":"x"}]}),
    ];
    let mut db = sqlite::Database::create(&path).unwrap();
    for row in &rows {
        db.insert(row.clone()).unwrap();
    }
    db.finish().unwrap();
    assert_rows(&Connection::open(path).unwrap(), &rows);
    let race = scratch.path().join("race.db");
    let db = sqlite::Database::create(&race).unwrap();
    std::fs::write(&race, "concurrent owner").unwrap();
    assert!(db.finish().is_err());
    assert_eq!(std::fs::read(race).unwrap(), b"concurrent owner");
}

#[test]
fn batches_cross_256_and_source_boundaries_without_secondary_indexes() {
    let scratch = tempfile::tempdir().unwrap();
    let path = scratch.path().join("batched.db");
    let mut db = sqlite::Database::create(&path).unwrap();
    db.source("a.rs", "digest-a".to_owned()).unwrap();
    for _ in 0..257 {
        db.insert(json!({"record":"protocol","version":1})).unwrap();
    }
    db.source("b.rs", "digest-b".to_owned()).unwrap();
    for _ in 0..44 {
        db.insert(json!({"record":"protocol","version":1})).unwrap();
    }
    db.finish().unwrap();

    let connection = Connection::open(path).unwrap();
    let coordinates = connection
        .prepare(
            "SELECT _row, _input_path, _content_id FROM protocol \
             WHERE _row IN (1, 256, 257, 258, 301) ORDER BY _row",
        )
        .unwrap()
        .query_map([], |row| {
            Ok((
                row.get::<_, i64>(0)?,
                row.get::<_, String>(1)?,
                row.get::<_, String>(2)?,
            ))
        })
        .unwrap()
        .collect::<rusqlite::Result<Vec<_>>>()
        .unwrap();
    assert_eq!(
        coordinates,
        vec![
            (1, "a.rs".to_owned(), "digest-a".to_owned()),
            (256, "a.rs".to_owned(), "digest-a".to_owned()),
            (257, "a.rs".to_owned(), "digest-a".to_owned()),
            (258, "b.rs".to_owned(), "digest-b".to_owned()),
            (301, "b.rs".to_owned(), "digest-b".to_owned()),
        ]
    );

    for table in tables() {
        let columns = connection
            .prepare(&format!("PRAGMA table_info({})", quote(&table.table)))
            .unwrap()
            .query_map([], |row| {
                Ok((
                    row.get::<_, String>(1)?,
                    row.get::<_, String>(2)?,
                    row.get::<_, i64>(5)?,
                ))
            })
            .unwrap()
            .collect::<rusqlite::Result<Vec<_>>>()
            .unwrap();
        let primary_keys: Vec<_> = columns
            .iter()
            .filter(|(_, _, primary_key)| *primary_key != 0)
            .cloned()
            .collect();
        assert_eq!(
            primary_keys,
            vec![("_row".to_owned(), "INTEGER".to_owned(), 1)]
        );

        let secondary_indexes: i64 = connection
            .query_row(
                "SELECT count(*) FROM pragma_index_list(?)",
                [&table.table],
                |row| row.get(0),
            )
            .unwrap();
        assert_eq!(secondary_indexes, 0, "{}", table.table);
    }
}
