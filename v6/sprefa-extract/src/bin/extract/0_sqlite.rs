//! SQLite export only. DDL, columns and wire paths come from TypeSpec.
//! A private staging database is published after commit, with no overwrite.
use std::io::{BufWriter, Write};
use std::path::{Path, PathBuf};
use std::time::Duration;

use rusqlite::Connection;
use serde::Serialize;
use serde_json::Value;

type Result<T> = std::result::Result<T, Box<dyn std::error::Error>>;
pub const DDL: &str = include_str!(concat!(
    env!("CARGO_MANIFEST_DIR"),
    "/../../schema/generated/4_facts.sql"
));
pub mod writers {
    include!(concat!(
        env!("CARGO_MANIFEST_DIR"),
        "/../../schema/generated/7_writers_auto.rs"
    ));
}

pub struct Database {
    connection: Connection,
    temporary: tempfile::NamedTempFile,
    destination: PathBuf,
    pub rows: i64,
    input_path: Option<String>,
    content_id: Option<String>,
    pending: Vec<writers::Fact>,
}

const BATCH_ROWS: usize = 256;

fn shell_quote(value: &str) -> String {
    format!("'{}'", value.replace('\'', "'\"'\"'"))
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
        connection.set_prepared_statement_cache_capacity(writers::TABLE_COUNT);
        connection.busy_timeout(Duration::from_secs(5))?;
        connection.execute_batch(
            "PRAGMA foreign_keys=ON; PRAGMA journal_mode=DELETE; BEGIN IMMEDIATE;",
        )?;
        connection.execute_batch(DDL)?;
        Ok(Self {
            connection,
            temporary,
            destination: std::path::absolute(path)?,
            rows: 0,
            input_path: None,
            content_id: None,
            pending: Vec::with_capacity(BATCH_ROWS),
        })
    }

    pub fn source(&mut self, path: &str, digest: String) -> Result<()> {
        if self.input_path.as_deref() == Some(path) && self.content_id.as_deref() == Some(&digest) {
            return Ok(());
        }
        self.flush_pending()?;
        self.input_path = Some(path.to_owned());
        self.content_id = Some(digest);
        Ok(())
    }

    pub fn clear_source(&mut self) -> Result<()> {
        if self.input_path.is_none() && self.content_id.is_none() {
            return Ok(());
        }
        self.flush_pending()?;
        self.input_path = None;
        self.content_id = None;
        Ok(())
    }

    pub fn insert(&mut self, value: Value) -> Result<()> {
        self.insert_fact(serde_json::from_value(value)?)
    }

    pub fn insert_fact(&mut self, fact: writers::Fact) -> Result<()> {
        self.rows = self
            .rows
            .checked_add(1)
            .ok_or("SQLite row counter overflow")?;
        self.pending.push(fact);
        if self.pending.len() == BATCH_ROWS {
            self.flush_pending()?;
        }
        Ok(())
    }

    fn flush_pending(&mut self) -> Result<()> {
        if self.pending.is_empty() {
            return Ok(());
        }
        let first_row = self.rows - i64::try_from(self.pending.len())? + 1;
        writers::insert_all(
            &self.connection,
            &writers::Source {
                row: first_row,
                input_path: self.input_path.as_deref(),
                content_id: self.content_id.as_deref(),
            },
            &self.pending,
        )?;
        self.pending.clear();
        Ok(())
    }

    pub fn finish(mut self) -> Result<()> {
        self.flush_pending()?;
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
            let fact = serde_json::from_str::<writers::Fact>(line)?;
            return db.insert_fact(fact);
        }
        self.stdout.write_all(line.as_bytes())?;
        self.stdout.write_all(b"\n")?;
        Ok(())
    }
    pub fn source_fact(
        &mut self,
        path: &str,
        content_id: &sprefa_extract::ContentId,
        fact: &impl Serialize,
    ) -> Result<()> {
        let db = self
            .database
            .as_mut()
            .ok_or("source facts require a SQLite output")?;
        if db.input_path.as_deref() != Some(path) {
            db.source(path, content_id.to_string())?;
        }
        db.insert_fact(serde_json::from_value(serde_json::to_value(fact)?)?)
    }
    pub fn clear_source(&mut self) -> Result<()> {
        if let Some(db) = &mut self.database {
            db.clear_source()?;
        }
        Ok(())
    }
    pub fn fact(&mut self, fact: &impl Serialize) -> Result<()> {
        if let Some(db) = &mut self.database {
            return db.insert_fact(serde_json::from_value(serde_json::to_value(fact)?)?);
        }
        serde_json::to_writer(&mut self.stdout, fact)?;
        self.stdout.write_all(b"\n")?;
        Ok(())
    }
    pub fn flush(&mut self) -> Result<()> {
        if let Some(db) = &mut self.database {
            db.flush_pending()?;
        }
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
