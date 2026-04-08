use sqlx::SqlitePool;

/// All migrations in order. Each is idempotent (IF NOT EXISTS).
const MIGRATIONS: &[&str] = &[
    // repos
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
    // files
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
        UNIQUE(repo_id, path, content_hash)
    )
    "#,
    // strings
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
    // FTS5 trigram index on strings.norm
    r#"
    CREATE VIRTUAL TABLE IF NOT EXISTS strings_fts USING fts5(
        norm,
        content='strings',
        content_rowid='id',
        tokenize='trigram'
    )
    "#,
    // FTS sync triggers
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
    // refs
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
    // rev_files junction (maps revs to their constituent files)
    r#"
    CREATE TABLE IF NOT EXISTS rev_files (
        repo_id INTEGER NOT NULL REFERENCES repos(id),
        rev TEXT NOT NULL,
        file_id INTEGER NOT NULL REFERENCES files(id),
        UNIQUE(repo_id, rev, file_id)
    )
    "#,
    // repo_revs (unified branch + tag tracking)
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
    // repo_packages
    r#"
    CREATE TABLE IF NOT EXISTS repo_packages (
        id INTEGER PRIMARY KEY AUTOINCREMENT,
        repo_id INTEGER NOT NULL REFERENCES repos(id),
        package_name TEXT NOT NULL,
        ecosystem TEXT NOT NULL,
        manifest_path TEXT NOT NULL,
        UNIQUE(repo_id, package_name, manifest_path)
    )
    "#,
    // sprf_meta: rule change detection for incremental extraction.
    // schema_hash = column definitions, extract_hash = pattern AST + producer hashes.
    r#"
    CREATE TABLE IF NOT EXISTS sprf_meta (
        rule_name TEXT PRIMARY KEY,
        source_file TEXT NOT NULL,
        schema_hash TEXT NOT NULL,
        extract_hash TEXT NOT NULL,
        last_scanned_at TEXT
    )
    "#,
    // Drop legacy matches table (replaced by per-rule _data tables).
    "DROP TABLE IF EXISTS matches",
    // discovery_log: records each (repo, rev) target found by tier 2 discovery.
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
    // invariant_violations: stores check block violations (one row per violation).
    r#"
    CREATE TABLE IF NOT EXISTS invariant_violations (
        id INTEGER PRIMARY KEY AUTOINCREMENT,
        check_name TEXT NOT NULL,
        violation_data TEXT NOT NULL,
        created_at TEXT NOT NULL DEFAULT (datetime('now')),
        resolved_at TEXT
    )
    "#,
];

/// Run all migrations against the given pool.
pub async fn run_migrations(pool: &SqlitePool) -> anyhow::Result<()> {
    // Enable WAL mode for concurrent reads
    sqlx::query("PRAGMA journal_mode=WAL")
        .execute(pool)
        .await?;
    sqlx::query("PRAGMA foreign_keys=ON")
        .execute(pool)
        .await?;

    for sql in MIGRATIONS {
        sqlx::query(sql).execute(pool).await?;
    }

    // Add scanner_hash to existing DBs that predate this column.
    // SQLite returns "duplicate column name" if it already exists -- ignore that.
    let _ = sqlx::query("ALTER TABLE files ADD COLUMN scanner_hash TEXT")
        .execute(pool)
        .await;

    // Add dir column to files for link predicates (DirEq).
    let _ = sqlx::query("ALTER TABLE files ADD COLUMN dir TEXT")
        .execute(pool)
        .await;

    tracing::info!("migrations complete ({} statements)", MIGRATIONS.len());
    Ok(())
}

/// Open (or create) a SQLite database and run migrations.
pub async fn init_db(path: &str) -> anyhow::Result<SqlitePool> {
    // Ensure parent directory exists
    if let Some(parent) = std::path::Path::new(path).parent() {
        std::fs::create_dir_all(parent)?;
    }

    let url = format!("sqlite://{}?mode=rwc", path);
    let pool = SqlitePool::connect(&url).await?;
    run_migrations(&pool).await?;
    Ok(pool)
}
