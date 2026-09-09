//! SQLite export only. DDL, columns and wire paths come from TypeSpec.
//! A private staging database is published after commit, with no overwrite.
use std::collections::HashMap;
use std::io::{BufWriter, Write};
use std::path::{Path, PathBuf};

use rusqlite::{params_from_iter, types::Value as SqlValue, Connection};
use serde::{Deserialize, Serialize};
use serde_json::Value;

type Result<T> = std::result::Result<T, Box<dyn std::error::Error>>;
pub const DDL: &str = include_str!(concat!(
    env!("CARGO_MANIFEST_DIR"),
    "/../../schema/generated/4_facts.sql"
));
pub const CATALOG: &str = include_str!(concat!(
    env!("CARGO_MANIFEST_DIR"),
    "/../../schema/generated/5_facts.json"
));

#[derive(Deserialize)]
pub struct Column {
    pub name: String,
    pub path: Vec<String>,
    pub kind: String,
    pub optional: bool,
    pub nullable: bool,
    pub literal: Option<String>,
    pub values: Option<Vec<String>>,
    pub json_type: Option<Value>,
}

#[derive(Deserialize)]
pub struct Table {
    pub record: String,
    pub table: String,
    pub columns: Vec<Column>,
    #[serde(default)]
    insert: String,
}

pub struct Database {
    connection: Connection,
    temporary: tempfile::NamedTempFile,
    destination: PathBuf,
    tables: HashMap<String, Table>,
    pub rows: i64,
    input_path: Option<String>,
    content_id: Option<String>,
}

fn quote(name: &str) -> String {
    format!("\"{}\"", name.replace('"', "\"\""))
}
fn shell_quote(value: &str) -> String {
    format!("'{}'", value.replace('\'', "'\"'\"'"))
}

// Only the finite payload shapes authored in TypeSpec. Open JSON is used solely
// for data_doc.doc; schema-generation rejects unsupported typed payloads.
fn json_matches(shape: &Value, value: &Value) -> bool {
    match shape["kind"].as_str() {
        Some("array") => value
            .as_array()
            .is_some_and(|vs| vs.iter().all(|v| json_matches(&shape["items"][0], v))),
        Some("tuple") => value.as_array().is_some_and(|vs| {
            let items = shape["items"].as_array().unwrap();
            items.len() == vs.len() && items.iter().zip(vs).all(|(s, v)| json_matches(s, v))
        }),
        Some("union") => shape["items"]
            .as_array()
            .unwrap()
            .iter()
            .any(|s| json_matches(s, value)),
        Some("object") => value.as_object().is_some_and(|obj| {
            let props = shape["properties"].as_array().unwrap();
            props.len() == obj.len()
                && props.iter().all(|p| {
                    obj.get(p["name"].as_str().unwrap())
                        .is_some_and(|v| json_matches(&p["type"], v))
                })
        }),
        Some("string") => value.is_string(),
        Some("boolean") => value.is_boolean(),
        Some("uint32") => value.as_u64().is_some_and(|n| u32::try_from(n).is_ok()),
        Some("uint64") => value.as_u64().is_some(),
        Some("int32") => value.as_i64().is_some_and(|n| i32::try_from(n).is_ok()),
        Some("int64") => value.as_i64().is_some(),
        _ => false,
    }
}

impl Database {
    pub fn create(path: &Path) -> Result<Self> {
        if path.as_os_str().is_empty() || path == Path::new(":memory:") {
            return Err("--sqlite requires a filesystem path for a new database".into());
        }
        if std::fs::symlink_metadata(path).is_ok() {
            return Err(format!(
                "--sqlite: {} already exists; supply a new database path",
                path.display()
            )
            .into());
        }
        let parent = path
            .parent()
            .filter(|p| !p.as_os_str().is_empty())
            .unwrap_or(Path::new("."));
        let temporary = tempfile::Builder::new()
            .prefix(".extract-sqlite-")
            .tempfile_in(parent)?;
        let connection = Connection::open(temporary.path())?;
        connection.execute_batch("PRAGMA journal_mode=DELETE; BEGIN IMMEDIATE;")?;
        connection.execute_batch(DDL)?;
        let tables = serde_json::from_str::<Vec<Table>>(CATALOG)?
            .into_iter()
            .map(|mut table| {
                table.insert = format!(
                    "INSERT INTO {} ({}) VALUES ({})",
                    quote(&table.table),
                    table
                        .columns
                        .iter()
                        .map(|c| quote(&c.name))
                        .collect::<Vec<_>>()
                        .join(","),
                    vec!["?"; table.columns.len()].join(",")
                );
                (table.record.clone(), table)
            })
            .collect();
        Ok(Self {
            connection,
            temporary,
            destination: std::path::absolute(path)?,
            tables,
            rows: 0,
            input_path: None,
            content_id: None,
        })
    }

    pub fn source(&mut self, path: &str, digest: String) {
        self.input_path = Some(path.to_owned());
        self.content_id = Some(digest);
    }

    pub fn insert(&mut self, value: Value) -> Result<()> {
        let object = value
            .as_object()
            .ok_or("SQLite export requires a fact object")?;
        let record = object
            .get("record")
            .and_then(Value::as_str)
            .ok_or("Fact has no record tag")?;
        let table = self
            .tables
            .get(record)
            .ok_or_else(|| format!("No TypeSpec SQL model for record {record}"))?;
        // A producer adding a field must update TypeSpec. Never drop unknown
        // fields or infer a replacement schema from whichever rows arrive first.
        fn check_keys(value: &Value, path: &mut Vec<String>, columns: &[Column]) -> Result<()> {
            if let Some(object) = value.as_object() {
                for (key, child) in object {
                    path.push(key.clone());
                    if path[0].starts_with('_') {
                        return Err(format!("Reserved export field: {}", path.join(".")).into());
                    }
                    if let Some(column) = columns.iter().find(|c| c.path == *path) {
                        if column.kind != "json" && child.is_object() {
                            return Err(format!("Unexpected object: {}", path.join(".")).into());
                        }
                    } else if columns.iter().any(|c| c.path.starts_with(path)) {
                        if !child.is_object() {
                            return Err(format!("Expected object: {}", path.join(".")).into());
                        }
                        check_keys(child, path, columns)?;
                    } else {
                        return Err(
                            format!("Field missing from TypeSpec: {}", path.join(".")).into()
                        );
                    }
                    path.pop();
                }
            }
            Ok(())
        }
        check_keys(&value, &mut Vec::new(), &table.columns)?;
        let next = self
            .rows
            .checked_add(1)
            .ok_or("SQLite row counter overflow")?;
        let values = table
            .columns
            .iter()
            .map(|column| -> Result<SqlValue> {
                match column.name.as_str() {
                    "_row" => return Ok(SqlValue::Integer(next)),
                    "_input_path" => {
                        return Ok(self
                            .input_path
                            .clone()
                            .map_or(SqlValue::Null, SqlValue::Text))
                    }
                    "_content_id" => {
                        return Ok(self
                            .content_id
                            .clone()
                            .map_or(SqlValue::Null, SqlValue::Text))
                    }
                    _ => {}
                }
                let found = column.path.iter().try_fold(&value, |v, key| v.get(key));
                let Some(found) = found else {
                    return if column.optional {
                        Ok(SqlValue::Null)
                    } else {
                        Err(
                            format!("Missing required field: {record}.{}", column.path.join("."))
                                .into(),
                        )
                    };
                };
                // JSON null is a JSON value for data_doc.doc, unlike SQL NULL.
                if column.kind == "json" {
                    if column
                        .json_type
                        .as_ref()
                        .is_some_and(|shape| !json_matches(shape, found))
                    {
                        return Err(
                            format!("Invalid JSON payload for {record}.{}", column.name).into()
                        );
                    }
                    return Ok(SqlValue::Text(serde_json::to_string(found)?));
                }
                if found.is_null() && column.nullable {
                    return Ok(SqlValue::Null);
                }
                let bad = || {
                    format!(
                        "Invalid {} for {record}.{}: {found}",
                        column.kind,
                        column.path.join(".")
                    )
                };
                if let Some(literal) = &column.literal {
                    if found.as_str() != Some(literal) {
                        return Err(bad().into());
                    }
                }
                if let Some(values) = &column.values {
                    if !found
                        .as_str()
                        .is_some_and(|v| values.iter().any(|allowed| allowed == v))
                    {
                        return Err(bad().into());
                    }
                }
                Ok(match column.kind.as_str() {
                    "string" => SqlValue::Text(found.as_str().ok_or_else(bad)?.to_owned()),
                    "boolean" => SqlValue::Integer(i64::from(found.as_bool().ok_or_else(bad)?)),
                    "uint32" => SqlValue::Integer(i64::from(
                        u32::try_from(found.as_u64().ok_or_else(bad)?).map_err(|_| bad())?,
                    )),
                    "uint64" => {
                        let n = found.as_u64().ok_or_else(bad)?;
                        // SQLite INTEGER is signed. Preserve larger u64 values as
                        // decimal TEXT through a BLOB-affinity column (see generator).
                        match i64::try_from(n) {
                            Ok(n) => SqlValue::Integer(n),
                            Err(_) => SqlValue::Text(n.to_string()),
                        }
                    }
                    "int32" => SqlValue::Integer(i64::from(
                        i32::try_from(found.as_i64().ok_or_else(bad)?).map_err(|_| bad())?,
                    )),
                    "int64" => SqlValue::Integer(found.as_i64().ok_or_else(bad)?),
                    _ => return Err(bad().into()),
                })
            })
            .collect::<Result<Vec<_>>>()?;
        self.connection
            .prepare_cached(&table.insert)?
            .execute(params_from_iter(values))?;
        self.rows = next;
        Ok(())
    }

    pub fn finish(self) -> Result<()> {
        self.connection.execute_batch("COMMIT;")?;
        self.connection.close().map_err(|(_, error)| error)?;
        self.temporary.as_file().sync_all()?;
        self.temporary.persist_noclobber(&self.destination)?;
        let path = shell_quote(&self.destination.to_string_lossy());
        let mut out = std::io::stdout().lock();
        writeln!(
            out,
            "Wrote {} ({} rows)",
            self.destination.display(),
            self.rows
        )?;
        writeln!(out, "Tables: sqlite3 {path} '.tables'")?;
        writeln!(out, "Schema: sqlite3 {path} '.schema'")?;
        writeln!(out, "Query:  sqlite3 -header -column {path} 'SELECT _input_path, family, kind, name FROM node LIMIT 20;'")?;
        Ok(())
    }
}

pub struct Output {
    pub database: Option<Database>,
    stdout: BufWriter<std::io::Stdout>,
}

impl Output {
    pub fn new(path: Option<&Path>) -> Result<Self> {
        Ok(Self {
            database: path.map(Database::create).transpose()?,
            stdout: BufWriter::with_capacity(256 * 1024, std::io::stdout()),
        })
    }
    pub fn line(&mut self, line: &str) -> Result<()> {
        if let Some(db) = &mut self.database {
            return db.insert(serde_json::from_str(line)?);
        }
        self.stdout.write_all(line.as_bytes())?;
        self.stdout.write_all(b"\n")?;
        Ok(())
    }
    pub fn fact(&mut self, fact: &impl Serialize) -> Result<()> {
        if let Some(db) = &mut self.database {
            return db.insert(serde_json::to_value(fact)?);
        }
        serde_json::to_writer(&mut self.stdout, fact)?;
        self.stdout.write_all(b"\n")?;
        Ok(())
    }
    pub fn flush(&mut self) -> Result<()> {
        self.stdout.flush()?;
        Ok(())
    }
    pub fn finish(mut self) -> Result<()> {
        self.stdout.flush()?;
        if let Some(db) = self.database {
            db.finish()?;
        }
        Ok(())
    }
}
