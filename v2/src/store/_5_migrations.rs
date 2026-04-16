//! Migrations and pool bootstrap for `SqliteStore`.
//!
//! Ported from `crates/schema/src/migrations.rs` with the v2 mutations
//! table added (effect cache for `record_effect` / `effect_status`).

use std::path::Path;

use sqlx::sqlite::{SqliteConnectOptions, SqlitePoolOptions};
use sqlx::{ConnectOptions, SqlitePool};

use super::_0_types::StoreErr;

const MIGRATIONS: &[&str] = &[
    r#"
    CREATE TABLE IF NOT EXISTS repos (
        id INTEGER PRIMARY KEY AUTOINCREMENT,
        name TEXT NOT NULL UNIQUE,
        root_path TEXT NOT NULL,
        org TEXT,
        git_hash TEXT,
        last_fetched_at TEXT,
        last_synced_at TEXT,
        last_remote_commit_at TEXT,
        scanned_at TEXT
    )
    "#,
    r#"
    CREATE TABLE IF NOT EXISTS files (
        id INTEGER PRIMARY KEY AUTOINCREMENT,
        repo_id INTEGER NOT NULL REFERENCES repos(id),
        path TEXT NOT NULL,
        content_hash TEXT NOT NULL,
        stem TEXT,
        ext TEXT,
        scanned_at TEXT,
        scanner_hash TEXT,
        dir TEXT,
        UNIQUE(repo_id, path, content_hash)
    )
    "#,
    r#"
    CREATE TABLE IF NOT EXISTS strings (
        id INTEGER PRIMARY KEY AUTOINCREMENT,
        value TEXT NOT NULL UNIQUE,
        norm TEXT,
        norm2 TEXT
    )
    "#,
    "CREATE INDEX IF NOT EXISTS idx_strings_norm ON strings(norm)",
    "CREATE INDEX IF NOT EXISTS idx_strings_norm2 ON strings(norm2)",
    r#"
    CREATE VIRTUAL TABLE IF NOT EXISTS strings_fts USING fts5(
        norm,
        content='strings',
        content_rowid='id',
        tokenize='trigram'
    )
    "#,
    r#"
    CREATE TRIGGER IF NOT EXISTS strings_ai AFTER INSERT ON strings BEGIN
        INSERT INTO strings_fts(rowid, norm)
        SELECT new.id, new.norm WHERE length(new.norm) < 1000;
    END
    "#,
    r#"
    CREATE TRIGGER IF NOT EXISTS strings_ad AFTER DELETE ON strings BEGIN
        INSERT INTO strings_fts(strings_fts, rowid, norm) VALUES('delete', old.id, old.norm);
    END
    "#,
    r#"
    CREATE TRIGGER IF NOT EXISTS strings_au AFTER UPDATE ON strings BEGIN
        INSERT INTO strings_fts(strings_fts, rowid, norm) VALUES('delete', old.id, old.norm);
        INSERT INTO strings_fts(rowid, norm)
        SELECT new.id, new.norm WHERE length(new.norm) < 1000;
    END
    "#,
    r#"
    CREATE TABLE IF NOT EXISTS refs (
        id INTEGER PRIMARY KEY AUTOINCREMENT,
        string_id INTEGER NOT NULL REFERENCES strings(id),
        file_id INTEGER NOT NULL REFERENCES files(id),
        span_start INTEGER NOT NULL,
        span_end INTEGER NOT NULL,
        is_path INTEGER NOT NULL DEFAULT 0,
        confidence REAL,
        target_file_id INTEGER REFERENCES files(id),
        ref_kind INTEGER NOT NULL DEFAULT 0,
        parent_key_string_id INTEGER REFERENCES strings(id),
        node_path TEXT,
        UNIQUE(file_id, string_id, span_start)
    )
    "#,
    "CREATE INDEX IF NOT EXISTS idx_refs_string_id ON refs(string_id)",
    "CREATE INDEX IF NOT EXISTS idx_refs_file_id ON refs(file_id)",
    "CREATE INDEX IF NOT EXISTS idx_refs_target_file_id ON refs(target_file_id)",
    r#"
    CREATE TABLE IF NOT EXISTS rev_files (
        repo_id INTEGER NOT NULL REFERENCES repos(id),
        rev TEXT NOT NULL,
        file_id INTEGER NOT NULL REFERENCES files(id),
        UNIQUE(repo_id, rev, file_id)
    )
    "#,
    r#"
    CREATE TABLE IF NOT EXISTS repo_revs (
        repo_id INTEGER NOT NULL REFERENCES repos(id),
        rev TEXT NOT NULL,
        git_hash TEXT,
        is_working_tree INTEGER NOT NULL DEFAULT 0,
        is_semver INTEGER NOT NULL DEFAULT 0,
        UNIQUE(repo_id, rev)
    )
    "#,
    r#"
    CREATE TABLE IF NOT EXISTS sprf_meta (
        expr_name TEXT PRIMARY KEY,
        schema_hash TEXT NOT NULL,
        extract_hash TEXT NOT NULL,
        last_scanned_at TEXT
    )
    "#,
    r#"
    CREATE TABLE IF NOT EXISTS mutations (
        id INTEGER PRIMARY KEY AUTOINCREMENT,
        kind_sigil TEXT NOT NULL,
        fingerprint TEXT NOT NULL,
        effect_hash TEXT NOT NULL,
        outcome TEXT NOT NULL,
        when_utc TEXT NOT NULL,
        UNIQUE(kind_sigil, fingerprint)
    )
    "#,
    "CREATE INDEX IF NOT EXISTS idx_mutations_fp ON mutations(fingerprint)",
    r#"
    CREATE TABLE IF NOT EXISTS discovery_log (
        id INTEGER PRIMARY KEY AUTOINCREMENT,
        iteration INTEGER NOT NULL,
        source_repo TEXT NOT NULL,
        source_file TEXT,
        source_kind TEXT,
        target_repo TEXT NOT NULL,
        target_rev TEXT NOT NULL,
        status TEXT NOT NULL,
        files_scanned INTEGER,
        refs_inserted INTEGER,
        created_at TEXT NOT NULL DEFAULT (datetime('now'))
    )
    "#,
];

pub async fn run_migrations(pool: &SqlitePool) -> Result<(), StoreErr> {
    for sql in MIGRATIONS {
        sqlx::query(sql)
            .execute(pool)
            .await
            .map_err(|e| StoreErr::Sql(e.to_string()))?;
    }
    Ok(())
}

pub async fn init_db(path: &Path) -> Result<SqlitePool, StoreErr> {
    if path != Path::new(":memory:") {
        if let Some(parent) = path.parent() {
            if !parent.as_os_str().is_empty() {
                std::fs::create_dir_all(parent)
                    .map_err(|e| StoreErr::Sql(format!("mkdir {}: {e}", parent.display())))?;
            }
        }
    }

    let is_memory = path == Path::new(":memory:");
    let mut opts = if is_memory {
        SqliteConnectOptions::new()
            .in_memory(true)
            .create_if_missing(true)
    } else {
        SqliteConnectOptions::new()
            .filename(path)
            .create_if_missing(true)
            .journal_mode(sqlx::sqlite::SqliteJournalMode::Wal)
            .synchronous(sqlx::sqlite::SqliteSynchronous::Normal)
            .foreign_keys(true)
    };
    opts = opts.disable_statement_logging();

    let max = if is_memory { 1 } else { 8 };
    let pool = SqlitePoolOptions::new()
        .max_connections(max)
        .after_connect(|conn, _meta| {
            Box::pin(async move {
                let mut handle = conn.lock_handle().await?;
                // Safety: lock_handle guarantees exclusive access to the
                // sqlite3* for the lock's lifetime.
                unsafe {
                    super::_4_udfs::register_all(handle.as_raw_handle().as_ptr());
                }
                drop(handle);
                Ok(())
            })
        })
        .connect_with(opts)
        .await
        .map_err(|e| StoreErr::Sql(e.to_string()))?;

    Ok(pool)
}
