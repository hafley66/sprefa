//! `PathIndex` — persistent (repo, rev) → [(path, blob_oid)] cache.
//!
//! Sits on the reader side; does not touch `crate::store::Store`.
//! `SqlxPathIndex` owns its own `SqlitePool` and DDL.
//! `NoopPathIndex` is the default in tests and when the feature is off.

use std::path::Path;
use std::sync::Arc;

use async_trait::async_trait;
use sqlx::sqlite::{SqliteConnectOptions, SqlitePoolOptions};
use sqlx::{ConnectOptions as _, SqlitePool};
use tracing::{info_span, Instrument};

use crate::types::FilePath;
use crate::pattern::CompiledPattern;

// ---------------------------------------------------------------------------
// Error
// ---------------------------------------------------------------------------

#[derive(Debug)]
pub enum PathIndexErr {
    Sql(String),
}

impl std::fmt::Display for PathIndexErr {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            PathIndexErr::Sql(s) => write!(f, "sqlx: {s}"),
        }
    }
}

impl std::error::Error for PathIndexErr {}

// ---------------------------------------------------------------------------
// Schema
// ---------------------------------------------------------------------------

const SCHEMA_SQL: &str = "
CREATE TABLE IF NOT EXISTS path_index (
    repo      TEXT NOT NULL,
    rev       TEXT NOT NULL,
    path      TEXT NOT NULL,
    blob_oid  BLOB NOT NULL,
    PRIMARY KEY (repo, rev, path)
) WITHOUT ROWID;

CREATE INDEX IF NOT EXISTS path_index_rev ON path_index(repo, rev);
";

// ---------------------------------------------------------------------------
// Trait
// ---------------------------------------------------------------------------

#[async_trait]
pub trait PathIndex: Send + Sync {
    /// `None` = rev not indexed; `Some(vec)` = authoritative (empty = no matches).
    async fn files_at(
        &self,
        repo: &str,
        rev: &str,
        pattern: &CompiledPattern,
    ) -> Result<Option<Vec<FilePath>>, PathIndexErr>;

    /// Bulk upsert: DELETE (repo, rev) then INSERT all entries in one tx.
    async fn upsert_rev(
        &self,
        repo: &str,
        rev: &str,
        entries: &[(FilePath, [u8; 20])],
    ) -> Result<(), PathIndexErr>;

    async fn drop_rev(&self, repo: &str, rev: &str) -> Result<(), PathIndexErr>;

    /// Cheap existence check; used by reader to branch phase 1 vs phase 2.
    async fn has_rev(&self, repo: &str, rev: &str) -> Result<bool, PathIndexErr>;
}

// ---------------------------------------------------------------------------
// SqlxPathIndex
// ---------------------------------------------------------------------------

pub struct SqlxPathIndex {
    pub pool: SqlitePool,
}

impl SqlxPathIndex {
    pub async fn open(path: &Path) -> Result<Arc<Self>, PathIndexErr> {
        let is_memory = path == Path::new(":memory:");

        let opts = if is_memory {
            SqliteConnectOptions::new()
                .in_memory(true)
                .create_if_missing(true)
        } else {
            if let Some(parent) = path.parent() {
                if !parent.as_os_str().is_empty() {
                    std::fs::create_dir_all(parent).map_err(|e| {
                        PathIndexErr::Sql(format!("mkdir {}: {e}", parent.display()))
                    })?;
                }
            }
            SqliteConnectOptions::new()
                .filename(path)
                .create_if_missing(true)
                .journal_mode(sqlx::sqlite::SqliteJournalMode::Wal)
                .synchronous(sqlx::sqlite::SqliteSynchronous::Normal)
        };
        let opts = opts.disable_statement_logging();

        let max = if is_memory { 1 } else { 8 };
        let pool = SqlitePoolOptions::new()
            .max_connections(max)
            .connect_with(opts)
            .instrument(info_span!("path_index.pool_connect"))
            .await
            .map_err(|e| PathIndexErr::Sql(e.to_string()))?;

        // Apply DDL inline — no migrations table needed.
        sqlx::query(SCHEMA_SQL)
            .execute(&pool)
            .instrument(info_span!("path_index.schema_ddl"))
            .await
            .map_err(|e| PathIndexErr::Sql(e.to_string()))?;

        Ok(Arc::new(Self { pool }))
    }

    pub async fn open_memory() -> Result<Arc<Self>, PathIndexErr> {
        Self::open(Path::new(":memory:")).await
    }
}

#[async_trait]
impl PathIndex for SqlxPathIndex {
    async fn files_at(
        &self,
        repo: &str,
        rev: &str,
        pattern: &CompiledPattern,
    ) -> Result<Option<Vec<FilePath>>, PathIndexErr> {
        // SQLite GLOB covers *, ?, [abc] but not brace expansion {a,b}.
        // TODO: detect brace expansion in pattern.src and skip GLOB push-down.
        let rows: Vec<(String,)> = sqlx::query_as(
            "SELECT path FROM path_index WHERE repo = ?1 AND rev = ?2 AND path GLOB ?3",
        )
        .bind(repo)
        .bind(rev)
        .bind(pattern.src.as_ref())
        .fetch_all(&self.pool)
        .instrument(info_span!("path_index.files_at.glob_query"))
        .await
        .map_err(|e| PathIndexErr::Sql(e.to_string()))?;

        // Hot path: non-empty result is authoritative — one round-trip.
        // Cold path (empty): disambiguate not-indexed vs indexed-but-no-match.
        if rows.is_empty() {
            if !self.has_rev(repo, rev)
                .instrument(info_span!("path_index.files_at.has_rev_fallback"))
                .await? {
                return Ok(None);
            }
            return Ok(Some(Vec::new()));
        }

        Ok(Some(
            rows.into_iter()
                .map(|(p,)| FilePath(Arc::from(Path::new(&p))))
                .collect(),
        ))
    }

    async fn upsert_rev(
        &self,
        repo: &str,
        rev: &str,
        entries: &[(FilePath, [u8; 20])],
    ) -> Result<(), PathIndexErr> {
        let mut tx = self
            .pool
            .begin()
            .instrument(info_span!("path_index.upsert_rev.tx_begin"))
            .await
            .map_err(|e| PathIndexErr::Sql(e.to_string()))?;

        sqlx::query("DELETE FROM path_index WHERE repo = ?1 AND rev = ?2")
            .bind(repo)
            .bind(rev)
            .execute(&mut *tx)
            .instrument(info_span!("path_index.upsert_rev.delete"))
            .await
            .map_err(|e| PathIndexErr::Sql(e.to_string()))?;

        // Multi-row VALUES. SQLite param cap is 32766; 4 cols/row → ≤8000/chunk.
        const CHUNK: usize = 8000;
        for chunk in entries.chunks(CHUNK) {
            if chunk.is_empty() {
                continue;
            }
            let mut qb: sqlx::QueryBuilder<sqlx::Sqlite> = sqlx::QueryBuilder::new(
                "INSERT INTO path_index(repo, rev, path, blob_oid) ",
            );
            qb.push_values(chunk, |mut b, (fp, oid)| {
                b.push_bind(repo)
                    .push_bind(rev)
                    .push_bind(fp.0.to_string_lossy().into_owned())
                    .push_bind(oid.to_vec());
            });
            qb.build()
                .execute(&mut *tx)
                .instrument(info_span!("path_index.upsert_rev.insert_chunk", n = chunk.len()))
                .await
                .map_err(|e| PathIndexErr::Sql(e.to_string()))?;
        }

        tx.commit()
            .instrument(info_span!("path_index.upsert_rev.commit"))
            .await
            .map_err(|e| PathIndexErr::Sql(e.to_string()))?;

        Ok(())
    }

    async fn drop_rev(&self, repo: &str, rev: &str) -> Result<(), PathIndexErr> {
        sqlx::query("DELETE FROM path_index WHERE repo = ?1 AND rev = ?2")
            .bind(repo)
            .bind(rev)
            .execute(&self.pool)
            .instrument(info_span!("path_index.drop_rev", repo = %repo, rev = %rev))
            .await
            .map_err(|e| PathIndexErr::Sql(e.to_string()))?;
        Ok(())
    }

    async fn has_rev(&self, repo: &str, rev: &str) -> Result<bool, PathIndexErr> {
        let row: Option<(i64,)> =
            sqlx::query_as("SELECT 1 FROM path_index WHERE repo = ?1 AND rev = ?2 LIMIT 1")
                .bind(repo)
                .bind(rev)
                .fetch_optional(&self.pool)
                .instrument(info_span!("path_index.has_rev"))
                .await
                .map_err(|e| PathIndexErr::Sql(e.to_string()))?;
        Ok(row.is_some())
    }
}

// ---------------------------------------------------------------------------
// NoopPathIndex
// ---------------------------------------------------------------------------

pub struct NoopPathIndex;

#[async_trait]
impl PathIndex for NoopPathIndex {
    async fn files_at(
        &self,
        _repo: &str,
        _rev: &str,
        _pattern: &CompiledPattern,
    ) -> Result<Option<Vec<FilePath>>, PathIndexErr> {
        Ok(None)
    }

    async fn upsert_rev(
        &self,
        _repo: &str,
        _rev: &str,
        _entries: &[(FilePath, [u8; 20])],
    ) -> Result<(), PathIndexErr> {
        Ok(())
    }

    async fn drop_rev(&self, _repo: &str, _rev: &str) -> Result<(), PathIndexErr> {
        Ok(())
    }

    async fn has_rev(&self, _repo: &str, _rev: &str) -> Result<bool, PathIndexErr> {
        Ok(false)
    }
}

// ---------------------------------------------------------------------------
// Tests
// ---------------------------------------------------------------------------

#[cfg(test)]
mod tests {
    use super::*;
    use std::path::Path;

    fn fp(s: &str) -> FilePath {
        FilePath(Arc::from(Path::new(s)))
    }

    fn zero_oid() -> [u8; 20] {
        [0u8; 20]
    }

    fn pat(s: &str) -> CompiledPattern {
        CompiledPattern::compile(s).expect("valid pattern")
    }

    #[tokio::test]
    async fn open_memory_schema_creates() {
        SqlxPathIndex::open_memory().await.expect("open_memory succeeds");
    }

    #[tokio::test]
    async fn upsert_then_files_at_roundtrip() {
        let idx = SqlxPathIndex::open_memory().await.unwrap();

        let entries = vec![
            (fp("src/main.rs"), zero_oid()),
            (fp("src/lib.rs"), zero_oid()),
            (fp("build.rs"), zero_oid()),
        ];
        idx.upsert_rev("myrepo", "abc123", &entries).await.unwrap();

        let result = idx
            .files_at("myrepo", "abc123", &pat("src/*.rs"))
            .await
            .unwrap();

        let mut paths: Vec<String> = result
            .unwrap()
            .into_iter()
            .map(|fp| fp.0.to_string_lossy().into_owned())
            .collect();
        paths.sort();

        assert_eq!(paths, vec!["src/lib.rs", "src/main.rs"]);
    }

    #[tokio::test]
    async fn has_rev_false_before_upsert_true_after() {
        let idx = SqlxPathIndex::open_memory().await.unwrap();

        assert!(!idx.has_rev("r", "rev1").await.unwrap());

        idx.upsert_rev("r", "rev1", &[(fp("a.rs"), zero_oid())])
            .await
            .unwrap();

        assert!(idx.has_rev("r", "rev1").await.unwrap());
    }

    #[tokio::test]
    async fn drop_rev_removes_all_for_rev() {
        let idx = SqlxPathIndex::open_memory().await.unwrap();

        let entries = vec![
            (fp("a.rs"), zero_oid()),
            (fp("b.rs"), zero_oid()),
        ];
        idx.upsert_rev("r", "rev1", &entries).await.unwrap();
        idx.upsert_rev("r", "rev2", &[(fp("c.rs"), zero_oid())])
            .await
            .unwrap();

        idx.drop_rev("r", "rev1").await.unwrap();

        assert!(!idx.has_rev("r", "rev1").await.unwrap());
        assert!(idx.has_rev("r", "rev2").await.unwrap());

        // rev1 files are gone; rev2 file remains
        let rev2 = idx
            .files_at("r", "rev2", &pat("*.rs"))
            .await
            .unwrap()
            .unwrap();
        let mut names: Vec<_> = rev2
            .into_iter()
            .map(|fp| fp.0.to_string_lossy().into_owned())
            .collect();
        names.sort();
        assert_eq!(names, vec!["c.rs"]);
    }
}
