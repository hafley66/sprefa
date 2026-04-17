# 6c — PathIndex trait + sqlx impl

Persistent (repo, rev) → [(path, blob_oid)] index. Query by pattern, upsert
from a full walk, drop a rev on invalidation.

## Placement

- New module: `v2/src/readers/_5_path_index.rs`
- New migration file next to it: `v2/src/readers/_5_path_index_schema.rs`
  (one `CREATE TABLE` string constant + one `CREATE INDEX`)
- Own `SqlitePool`, own connect+migrate function. Does not touch
  `crate::store::Store` trait, does not extend `SqliteStore`.

Rationale: Store owns per-expr rows + effect cache. PathIndex is a
reader-side cache. Keeping them separate avoids extending the `Store`
trait (deep refactor) and keeps pool lifetimes independent.

## Trait

```rust
# v2/src/readers/_5_path_index.rs
use crate::_0_types::FilePath;
use crate::walk::CompiledPattern;   # or wherever the glob type lives
use async_trait::async_trait;
use std::sync::Arc;

#[async_trait]
pub trait PathIndex: Send + Sync {
    /// None = rev not indexed; Some(vec) = authoritative (empty vec = no
    /// matches at that rev).
    async fn files_at(
        &self,
        repo: &str, rev: &str, pattern: &CompiledPattern,
    ) -> Result<Option<Vec<FilePath>>, PathIndexErr>;

    /// Bulk upsert: DELETE (repo,rev) then INSERT all entries in one tx.
    async fn upsert_rev(
        &self,
        repo: &str, rev: &str, entries: &[(FilePath, [u8; 20])],
    ) -> Result<(), PathIndexErr>;

    async fn drop_rev(&self, repo: &str, rev: &str)
        -> Result<(), PathIndexErr>;

    /// Cheap existence check; used by reader to branch phase 1 vs phase 2.
    async fn has_rev(&self, repo: &str, rev: &str)
        -> Result<bool, PathIndexErr>;
}

#[derive(Debug, thiserror::Error)]
pub enum PathIndexErr {
    #[error("sqlx: {0}")] Sql(String),
}
```

## Schema

```sql
CREATE TABLE IF NOT EXISTS path_index (
    repo      TEXT NOT NULL,
    rev       TEXT NOT NULL,
    path      TEXT NOT NULL,
    blob_oid  BLOB NOT NULL,        -- 20 bytes
    PRIMARY KEY (repo, rev, path)
) WITHOUT ROWID;

CREATE INDEX IF NOT EXISTS path_index_rev ON path_index(repo, rev);
-- GLOB on path scans within (repo, rev) partition; fine for swc scale
```

## sqlx impl shape (mirrors `store::_2_sqlite::SqliteStore::open`)

```rust
pub struct SqlxPathIndex {
    pub pool: sqlx::SqlitePool,
}

impl SqlxPathIndex {
    pub async fn open(path: &std::path::Path)
        -> Result<Arc<Self>, PathIndexErr>
    {
        # Same two-step as SqliteStore::open: init_db then run_migrations.
        # Inline here — do not add to crate::store::_5_migrations.
        let pool = sqlx::SqlitePool::connect(/* sqlite path */).await
            .map_err(|e| PathIndexErr::Sql(e.to_string()))?;
        sqlx::query(SCHEMA_SQL).execute(&pool).await
            .map_err(|e| PathIndexErr::Sql(e.to_string()))?;
        Ok(Arc::new(Self { pool }))
    }

    pub async fn open_memory() -> Result<Arc<Self>, PathIndexErr> {
        Self::open(std::path::Path::new(":memory:")).await
    }
}

#[async_trait]
impl PathIndex for SqlxPathIndex {
    async fn files_at(&self, repo, rev, pattern) -> _ {
        if !self.has_rev(repo, rev).await? { return Ok(None); }
        let rows: Vec<(String,)> = sqlx::query_as(
            "SELECT path FROM path_index
             WHERE repo = ?1 AND rev = ?2 AND path GLOB ?3")
            .bind(repo).bind(rev).bind(pattern.src.as_ref())
            .fetch_all(&self.pool).await
            .map_err(|e| PathIndexErr::Sql(e.to_string()))?;
        Ok(Some(rows.into_iter()
            .map(|(p,)| FilePath(Arc::from(std::path::Path::new(&p))))
            .collect()))
    }

    async fn upsert_rev(&self, repo, rev, entries) -> _ {
        let mut tx = self.pool.begin().await
            .map_err(|e| PathIndexErr::Sql(e.to_string()))?;
        sqlx::query("DELETE FROM path_index WHERE repo=?1 AND rev=?2")
            .bind(repo).bind(rev)
            .execute(&mut *tx).await
            .map_err(|e| PathIndexErr::Sql(e.to_string()))?;
        for (fp, oid) in entries {
            sqlx::query(
                "INSERT INTO path_index(repo,rev,path,blob_oid)
                 VALUES (?1,?2,?3,?4)")
                .bind(repo).bind(rev)
                .bind(fp.as_str()).bind(&oid[..])
                .execute(&mut *tx).await
                .map_err(|e| PathIndexErr::Sql(e.to_string()))?;
        }
        tx.commit().await.map_err(|e| PathIndexErr::Sql(e.to_string()))?;
        Ok(())
    }

    async fn drop_rev(&self, repo, rev) -> _ {
        sqlx::query("DELETE FROM path_index WHERE repo=?1 AND rev=?2")
            .bind(repo).bind(rev)
            .execute(&self.pool).await
            .map_err(|e| PathIndexErr::Sql(e.to_string()))?;
        Ok(())
    }

    async fn has_rev(&self, repo, rev) -> _ {
        let row: Option<(i64,)> = sqlx::query_as(
            "SELECT 1 FROM path_index WHERE repo=?1 AND rev=?2 LIMIT 1")
            .bind(repo).bind(rev)
            .fetch_optional(&self.pool).await
            .map_err(|e| PathIndexErr::Sql(e.to_string()))?;
        Ok(row.is_some())
    }
}
```

## NoopPathIndex (for tests / feature-off)

```rust
pub struct NoopPathIndex;

#[async_trait]
impl PathIndex for NoopPathIndex {
    async fn files_at(&self,_,_,_) -> _ { Ok(None) }
    async fn upsert_rev(&self,_,_,_) -> _ { Ok(()) }
    async fn drop_rev(&self,_,_) -> _ { Ok(()) }
    async fn has_rev(&self,_,_) -> _ { Ok(false) }
}
```

`OpCtx::for_test` gets `Arc<NoopPathIndex>` by default.

## Pattern → GLOB

- `CompiledPattern` is globset-based
- SQLite `GLOB` supports `*`, `?`, `[abc]` — same subset
- Edge cases (`{a,b}` brace expansion): fall back in `files_at` to
  `SELECT path FROM path_index WHERE repo=? AND rev=?` then filter
  in-process

## Blast radius (hard cap)

- `v2/src/readers/_5_path_index.rs` — new, trait + `SqlxPathIndex` +
  `NoopPathIndex`, ~180 lines
- `v2/src/readers/mod.rs` — `pub mod _5_path_index; pub use ...;`
- Tests: open_memory round-trip (upsert → files_at → drop_rev → has_rev)

Out: no changes to `crate::store`. No changes to `Config`/`RuntimeConfig`
in this task (6e wires it). No changes to `GitBlobReader` (6d wires it).

## Stop conditions

Halt and report if any of these would be needed to finish:
- Adding methods to `crate::store::Store` or `SqliteStore`
- Touching `OpCtx::for_test` beyond one `Arc<NoopPathIndex>` field
- Changing `CompiledPattern` or `FilePath` shapes
- Adding dependencies to `v2/Cargo.toml` other than what's already there
  (`sqlx`, `async-trait`, `thiserror` are already in the workspace)

## Depends on / depended on by

- Independent. Lands standalone with unit tests.
- 6d consumes `PathIndex` from reader. 6e wires `SqlxPathIndex` via
  `RuntimeConfig::path_index_db_path`. 6f calls `drop_rev` from watcher.
