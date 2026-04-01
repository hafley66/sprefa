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
    // repo_refs: repo-level metadata strings (repo name, git tags, branches).
    // These participate in the match/link system without a file anchor.
    r#"
    CREATE TABLE IF NOT EXISTS repo_refs (
        id INTEGER PRIMARY KEY AUTOINCREMENT,
        string_id INTEGER NOT NULL REFERENCES strings(id),
        repo_id INTEGER NOT NULL REFERENCES repos(id),
        kind TEXT NOT NULL,
        UNIQUE(repo_id, string_id, kind)
    )
    "#,
    "CREATE INDEX IF NOT EXISTS idx_repo_refs_string_id ON repo_refs(string_id)",
    "CREATE INDEX IF NOT EXISTS idx_repo_refs_repo_id ON repo_refs(repo_id)",
    // matches: semantic interpretation of physical refs or repo-level metadata.
    // Exactly one of (ref_id, repo_ref_id) is non-null.
    // kind is a free-text string (no enum), rule_name identifies which rule produced it.
    r#"
    CREATE TABLE IF NOT EXISTS matches (
        id INTEGER PRIMARY KEY AUTOINCREMENT,
        ref_id INTEGER REFERENCES refs(id),
        repo_ref_id INTEGER REFERENCES repo_refs(id),
        rule_name TEXT NOT NULL,
        kind TEXT NOT NULL,
        CHECK((ref_id IS NULL) != (repo_ref_id IS NULL))
    )
    "#,
    "CREATE UNIQUE INDEX IF NOT EXISTS idx_matches_file_unique ON matches(ref_id, rule_name, kind) WHERE ref_id IS NOT NULL",
    "CREATE UNIQUE INDEX IF NOT EXISTS idx_matches_repo_unique ON matches(repo_ref_id, rule_name, kind) WHERE repo_ref_id IS NOT NULL",
    "CREATE INDEX IF NOT EXISTS idx_matches_ref_id ON matches(ref_id)",
    "CREATE INDEX IF NOT EXISTS idx_matches_repo_ref_id ON matches(repo_ref_id)",
    "CREATE INDEX IF NOT EXISTS idx_matches_kind ON matches(kind)",
    "CREATE INDEX IF NOT EXISTS idx_matches_rule_name ON matches(rule_name)",
    // match_labels: arbitrary key-value metadata on semantic matches
    r#"
    CREATE TABLE IF NOT EXISTS match_labels (
        match_id INTEGER NOT NULL REFERENCES matches(id),
        key TEXT NOT NULL,
        value TEXT NOT NULL,
        UNIQUE(match_id, key)
    )
    "#,
    "CREATE INDEX IF NOT EXISTS idx_match_labels_match_id ON match_labels(match_id)",
    // match_links: cross-file semantic links between matches.
    // e.g. import_name in file A -> export_name in file B.
    // Additive-only table: can DROP + recreate without touching refs/matches.
    r#"
    CREATE TABLE IF NOT EXISTS match_links (
        id INTEGER PRIMARY KEY AUTOINCREMENT,
        source_match_id INTEGER NOT NULL REFERENCES matches(id),
        target_match_id INTEGER NOT NULL REFERENCES matches(id),
        link_kind TEXT NOT NULL,
        UNIQUE(source_match_id, target_match_id, link_kind)
    )
    "#,
    "CREATE INDEX IF NOT EXISTS idx_match_links_source ON match_links(source_match_id)",
    "CREATE INDEX IF NOT EXISTS idx_match_links_target ON match_links(target_match_id)",
    "CREATE INDEX IF NOT EXISTS idx_match_links_kind ON match_links(link_kind)",
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

    // If matches table has old shape (ref_id NOT NULL, no repo_ref_id), rebuild it.
    // The DB is a rebuildable cache so data loss is acceptable.
    let has_repo_ref_id: i64 = sqlx::query_scalar::<_, i64>(
        "SELECT COUNT(*) FROM pragma_table_info('matches') WHERE name = 'repo_ref_id'",
    )
    .fetch_one(pool)
    .await
    .unwrap_or(0);
    if has_repo_ref_id == 0 {
        // Old schema -- drop match_links first (FK dependency), then matches,
        // then let the CREATE TABLE IF NOT EXISTS statements above recreate both.
        let _ = sqlx::query("DROP TABLE IF EXISTS match_links").execute(pool).await;
        let _ = sqlx::query("DROP TABLE IF EXISTS match_labels").execute(pool).await;
        let _ = sqlx::query("DROP TABLE IF EXISTS matches").execute(pool).await;
        for sql in MIGRATIONS {
            if sql.contains("matches") || sql.contains("match_links") || sql.contains("match_labels") {
                let _ = sqlx::query(sql).execute(pool).await;
            }
        }
    }



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
