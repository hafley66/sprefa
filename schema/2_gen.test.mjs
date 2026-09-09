import assert from "node:assert/strict";
import { execFileSync } from "node:child_process";
import { mkdtemp, readFile, rm, writeFile } from "node:fs/promises";
import { tmpdir } from "node:os";
import { join } from "node:path";
import test from "node:test";
import { checkGenerated, generate, repository, schemaDirectory } from "./2_gen.mjs";

const files = await generate();

test("generation is deterministic and checked-in artifacts are current", async () => {
  assert.deepEqual(await generate(), files);
  await checkGenerated(files);
});

test("TypeSpec span fields match the existing Rust contracts", async () => {
  const source = await readFile(join(repository, "v6/sprefa-extract/src/types.rs"), "utf8");
  const models = JSON.parse(files.get("3_models.json"));
  const rustScalars = { uint32: "u32" };
  for (const model of models) {
    const declaration = source.match(new RegExp(`pub struct ${model.name} \\{([^}]+)\\}`));
    assert.ok(declaration, `Existing Rust type ${model.name}`);
    const actual = [...declaration[1].matchAll(/pub (\w+): (\w+)/g)].map(([, name, type]) => [name, type]);
    assert.deepEqual(actual, model.fields.map(field => [field.name, rustScalars[field.type.name]]));
  }
});

test("generated SQLite tables accept span fixtures and enforce their row keys", () => {
  const sql = files.get("2_sql_trial.sql");
  const result = execFileSync("sqlite3", ["-batch", "-bail", ":memory:"], {
    encoding: "utf8", timeout: 10_000,
    input: `${sql}
      INSERT INTO span_row VALUES (1, 0, 0), (2, 7, 3), (3, 4294967295, 0);
      INSERT INTO span_out_row VALUES (1, 0, 0), (2, 7, 10), (3, 4294967295, 4294967295);
      SELECT 'local', id, start, len FROM span_row ORDER BY id;
      SELECT 'wire', id, start, end FROM span_out_row ORDER BY id;
      PRAGMA integrity_check;
    `,
  });
  assert.equal(result, "local|1|0|0\nlocal|2|7|3\nlocal|3|4294967295|0\nwire|1|0|0\nwire|2|7|10\nwire|3|4294967295|4294967295\nok\n");
  assert.throws(() => execFileSync("sqlite3", ["-batch", "-bail", ":memory:"], {
    encoding: "utf8", timeout: 10_000, stdio: ["pipe", "pipe", "pipe"],
    input: `${sql}\nINSERT INTO span_row VALUES (1, 0, 0), (1, 7, 3);`,
  }), /UNIQUE constraint failed/);
});

test("malformed or unsupported TypeSpec fails before artifact publication", async () => {
  const scratch = await mkdtemp(join(tmpdir(), "sprefa-schema-errors-"));
  try {
    const entry = join(scratch, "invalid.tsp");
    await writeFile(entry, "namespace Extract; model Broken { value: MissingType; }");
    await assert.rejects(generate(entry), /MissingType/);
    await writeFile(entry, "namespace Extract; model Broken { value: string; }");
    await assert.rejects(generate(entry), /Unsupported span field/);
    await checkGenerated(files);
  } finally {
    await rm(scratch, { recursive: true, force: true });
  }
});

test("generated Rust compiles, preserves u32 values, and executes native writers inside the caller transaction", async () => {
  const scratch = await mkdtemp(join(tmpdir(), "sprefa-schema-rust-"));
  try {
    for (const [name, content] of files) if (name.endsWith(".rs")) await writeFile(join(scratch, name), content);
    // Reuse the version already locked by extract. No production dependency change.
    const lock = await readFile(join(repository, "v6/sprefa-extract/Cargo.lock"), "utf8");
    const version = lock.match(/name = "serde"\nversion = "([^"]+)"/)[1];
    const rusqliteVersion = lock.match(/name = "rusqlite"\nversion = "([^"]+)"/)[1];
    const serdeJsonVersion = lock.match(/name = "serde_json"\nversion = "([^"]+)"/)[1];
    await writeFile(join(scratch, "Cargo.toml"), `[package]\nname = "sprefa-schema-trial"\nversion = "0.0.0"\nedition = "2021"\n[lib]\npath = "lib.rs"\n[dependencies]\nserde = { version = "=${version}", features = ["derive"] }\nserde_json = "=${serdeJsonVersion}"\nrusqlite = { version = "=${rusqliteVersion}", features = ["bundled", "limits", "trace"] }\n[workspace]\n`);
    await writeFile(join(scratch, "4_facts.sql"), files.get("4_facts.sql"));
    await writeFile(join(scratch, "lib.rs"), `
      #[path = "0_span.rs"] pub mod local;
      #[path = "1_span_out.rs"] pub mod wire;
      #[path = "7_writers_auto.rs"] pub mod writers;
      use std::sync::atomic::{AtomicUsize, Ordering};
      static INSERT_STATEMENTS: AtomicUsize = AtomicUsize::new(0);
      fn count_inserts(sql: &str) { if sql.starts_with("INSERT INTO") { INSERT_STATEMENTS.fetch_add(1, Ordering::Relaxed); } }
      #[test] fn values() {
        let local = local::Span { start: u32::MAX, len: 0 };
        let wire = wire::SpanOut { start: local.start, end: local.start + local.len };
        assert_eq!((wire.start, wire.end), (u32::MAX, u32::MAX));
      }
      #[test] fn native_writers() {
        use rusqlite::Connection;
        let mut conn = Connection::open_in_memory().unwrap();
        conn.set_prepared_statement_cache_capacity(writers::TABLE_COUNT);
        conn.execute_batch(include_str!("4_facts.sql")).unwrap();
        let tx = conn.transaction().unwrap();
        let source = writers::Source { row: 1, input_path: None, content_id: None };
        let protocol = writers::models::Protocol { version: u32::MAX };
        assert_eq!(protocol.insert(&tx, &source).unwrap(), 1);
        assert_eq!(tx.query_row("SELECT version FROM protocol", [], |r| r.get::<_, u32>(0)).unwrap(), u32::MAX);
        assert!(!tx.is_autocommit());
        tx.rollback().unwrap();
        assert_eq!(conn.query_row("SELECT count(*) FROM protocol", [], |r| r.get::<_, i64>(0)).unwrap(), 0);
      }
      #[test] fn tagged_rows_preserve_presence_json_spans_u64_and_batch_rollback() {
        use rusqlite::Connection;
        let mut conn = Connection::open_in_memory().unwrap();
        conn.set_prepared_statement_cache_capacity(writers::TABLE_COUNT);
        conn.execute_batch(include_str!("4_facts.sql")).unwrap();
        assert_eq!(conn.query_row("SELECT count(*) FROM sqlite_master WHERE type = 'index' AND sql IS NOT NULL", [], |r| r.get::<_,i64>(0)).unwrap(), 0);

        let size: writers::Fact = serde_json::from_value(serde_json::json!({
          "record":"size_skip", "path":"p", "bytes":u64::MAX, "limit":9223372036854775808u64, "reason":"large"
        })).unwrap();
        size.insert(&conn, &writers::Source { row: 1, input_path: Some("p"), content_id: Some("d") }).unwrap();
        assert_eq!(conn.query_row("SELECT typeof(bytes), bytes, typeof(\\\"limit\\\"), \\\"limit\\\" FROM size_skip", [], |r| Ok((r.get::<_,String>(0)?,r.get::<_,String>(1)?,r.get::<_,String>(2)?,r.get::<_,String>(3)?))).unwrap(),
          ("text".into(), u64::MAX.to_string(), "text".into(), "9223372036854775808".into()));

        let json_null: writers::Fact = serde_json::from_value(serde_json::json!({
          "record":"data_doc", "family":"data", "ordinal":0, "span":{"start":2,"end":3}, "format":"json", "doc":null
        })).unwrap();
        json_null.insert(&conn, &writers::Source { row: 2, input_path: None, content_id: None }).unwrap();
        assert_eq!(conn.query_row("SELECT doc FROM data_doc", [], |r| r.get::<_,String>(0)).unwrap(), "null");

        let nullable: writers::Fact = serde_json::from_value(serde_json::json!({
          "record":"scip_index", "reused":false, "tool_name":"x", "tool_version":"1", "documents":0,
          "index_mtime_unix_ms":null, "staleness":"fresh"
        })).unwrap();
        nullable.insert(&conn, &writers::Source { row: 3, input_path: None, content_id: None }).unwrap();
        assert!(serde_json::from_value::<writers::Fact>(serde_json::json!({
          "record":"scip_index", "reused":false, "tool_name":"x", "tool_version":"1", "documents":0, "staleness":"fresh"
        })).is_err());
        assert!(serde_json::from_value::<writers::Fact>(serde_json::json!({
          "record":"node", "fact":null, "family":"cst", "span":{"start":0,"end":1}, "kind":"id", "name":null
        })).is_err());
        assert!(serde_json::from_value::<writers::Fact>(serde_json::json!({
          "record":"node", "family":"cst", "span":{"start":0,"end":1,"extra":2}, "kind":"id", "name":null
        })).is_err());
        let doc: writers::Fact = serde_json::from_value(serde_json::json!({
          "record":"doc_node", "family":"data", "span":{"start":0,"end":4}, "kind":"section", "name":"n", "parent":null,
          "body":{"start":1,"end":3}
        })).unwrap();
        assert_eq!(serde_json::to_value(&doc).unwrap()["body"]["end"], 3);

        let mixed = [
          serde_json::from_value(serde_json::json!({"record":"protocol","version":2})).unwrap(),
          serde_json::from_value(serde_json::json!({"record":"size_skip","path":"a","bytes":1,"limit":2,"reason":"r"})).unwrap(),
          serde_json::from_value(serde_json::json!({"record":"protocol","version":3})).unwrap(),
          serde_json::from_value(serde_json::json!({"record":"protocol","version":4})).unwrap(),
          serde_json::from_value(serde_json::json!({"record":"size_skip","path":"b","bytes":3,"limit":4,"reason":"r"})).unwrap(),
        ];
        conn.set_limit(rusqlite::limits::Limit::SQLITE_LIMIT_VARIABLE_NUMBER, 10).unwrap();
        assert_eq!(writers::max_batch_rows(&conn).unwrap(), 2);
        INSERT_STATEMENTS.store(0, Ordering::Relaxed);
        conn.trace(Some(count_inserts));
        assert_eq!(writers::insert_all(&conn, &writers::Source { row: 10, input_path: None, content_id: None }, &mixed).unwrap(), 5);
        conn.trace(None);
        assert_eq!(INSERT_STATEMENTS.load(Ordering::Relaxed), 4);
        assert_eq!(conn.query_row("SELECT group_concat(_row, ',') FROM protocol WHERE _row >= 10", [], |r| r.get::<_,String>(0)).unwrap(), "10,12,13");
        assert_eq!(conn.query_row("SELECT group_concat(_row, ',') FROM size_skip WHERE _row >= 10", [], |r| r.get::<_,String>(0)).unwrap(), "11,14");

        const PROTOCOL_PREFIX: &str = "INSERT INTO \\"protocol\\" (\\"_row\\", \\"_input_path\\", \\"_content_id\\", \\"record\\", \\"version\\") VALUES ";
        const PROTOCOL_TUPLE: &str = "(?, ?, ?, ?, ?)";
        let protocol_one = PROTOCOL_PREFIX.len() + PROTOCOL_TUPLE.len();
        let protocol_two = protocol_one + 2 + PROTOCOL_TUPLE.len();
        let old_sql_limit = conn.set_limit(rusqlite::limits::Limit::SQLITE_LIMIT_SQL_LENGTH, i32::try_from(protocol_one).unwrap()).unwrap();
        assert_eq!(writers::max_batch_rows(&conn).unwrap(), 1);
        assert_eq!(writers::insert_all(&conn, &writers::Source { row: 30, input_path: None, content_id: None }, &mixed[..1]).unwrap(), 1);

        conn.set_limit(rusqlite::limits::Limit::SQLITE_LIMIT_SQL_LENGTH, i32::try_from(protocol_two).unwrap()).unwrap();
        assert_eq!(writers::max_batch_rows(&conn).unwrap(), 2);
        INSERT_STATEMENTS.store(0, Ordering::Relaxed);
        conn.trace(Some(count_inserts));
        assert_eq!(writers::insert_all(&conn, &writers::Source { row: 31, input_path: None, content_id: None }, &[mixed[0].clone(), mixed[2].clone(), mixed[3].clone()]).unwrap(), 3);
        conn.trace(None);
        assert_eq!(INSERT_STATEMENTS.load(Ordering::Relaxed), 2);

        conn.set_limit(rusqlite::limits::Limit::SQLITE_LIMIT_SQL_LENGTH, i32::try_from(protocol_one - 1).unwrap()).unwrap();
        let before = conn.total_changes();
        assert!(matches!(writers::insert_all(&conn, &writers::Source { row: 40, input_path: None, content_id: None }, &mixed[..1]), Err(writers::InsertError::SQLiteLimit(_))));
        assert_eq!(conn.total_changes(), before);

        conn.set_limit(rusqlite::limits::Limit::SQLITE_LIMIT_SQL_LENGTH, i32::try_from(protocol_one).unwrap()).unwrap();
        let before = conn.total_changes();
        assert!(matches!(writers::insert_all(&conn, &writers::Source { row: 40, input_path: None, content_id: None }, &mixed[..2]), Err(writers::InsertError::SQLiteLimit(_))));
        assert_eq!(conn.total_changes(), before);
        assert_eq!(conn.query_row("SELECT count(*) FROM protocol WHERE _row = 40", [], |r| r.get::<_,i64>(0)).unwrap(), 0);
        conn.set_limit(rusqlite::limits::Limit::SQLITE_LIMIT_SQL_LENGTH, old_sql_limit).unwrap();

        writers::models::Protocol { version: 1 }.insert(&conn, &writers::Source { row: 22, input_path: None, content_id: None }).unwrap();
        let batch = [
          serde_json::from_value(serde_json::json!({"record":"protocol","version":2})).unwrap(),
          serde_json::from_value(serde_json::json!({"record":"protocol","version":3})).unwrap(),
          serde_json::from_value(serde_json::json!({"record":"protocol","version":4})).unwrap(),
        ];
        let tx = conn.transaction().unwrap();
        assert!(writers::insert_all(&tx, &writers::Source { row: 20, input_path: None, content_id: None }, &batch).is_err());
        tx.rollback().unwrap();
        assert_eq!(conn.query_row("SELECT count(*) FROM protocol WHERE _row IN (20, 21)", [], |r| r.get::<_,i64>(0)).unwrap(), 0);
        let before = conn.total_changes();
        assert!(matches!(writers::insert_all(&conn, &writers::Source { row: i64::MAX, input_path: None, content_id: None }, &batch), Err(writers::InsertError::OrdinalOverflow)));
        assert_eq!(conn.total_changes(), before);
      }
    `);
    const result = execFileSync("cargo", ["test", "--offline", "--manifest-path", join(scratch, "Cargo.toml")], {
      cwd: scratch, encoding: "utf8", timeout: 90_000,
      env: { ...process.env, CARGO_TARGET_DIR: process.env.SPREFA_SCHEMA_CARGO_TARGET_DIR ?? join(scratch, "target") },
      stdio: ["ignore", "pipe", "pipe"],
    });
    assert.match(result, /3 passed; 0 failed/);
  } finally {
    await rm(scratch, { recursive: true, force: true });
  }
});
