# 6c — PathIndex trait + SqliteStore impl

Persistent (repo, rev) → [(path, blob_oid)] index. Query by pattern, upsert
from a full walk, drop a rev on invalidation.

## Trait

```rust
# v2/src/store/_1_path_index.rs
pub trait PathIndex: Send + Sync {
    fn files_at(
        &self,
        repo: &str, rev: &str, pattern: &CompiledPattern,
    ) -> Option<Vec<FilePath>>;        # None = not indexed yet

    fn upsert_rev(
        &self,
        repo: &str, rev: &str, entries: &[(FilePath, [u8; 20])],
    );

    fn drop_rev(&self, repo: &str, rev: &str);

    fn has_rev(&self, repo: &str, rev: &str) -> bool;
    # cheap check for branch logic in reader
}
```

## Schema

```sql
CREATE TABLE IF NOT EXISTS blob_index (
    repo      TEXT  NOT NULL,
    rev       TEXT  NOT NULL,
    path      TEXT  NOT NULL,
    blob_oid  BLOB  NOT NULL,              -- 20 bytes
    PRIMARY KEY (repo, rev, path)
) WITHOUT ROWID;

CREATE INDEX IF NOT EXISTS blob_index_rev ON blob_index(repo, rev);
-- GLOB on path uses scan within (repo, rev) partition; fine for swc scale
```

## SqliteStore impl shape

```rust
impl PathIndex for SqliteStore {
    fn files_at(&self, repo, rev, pattern) -> Option<Vec<FilePath>> {
        if !self.has_rev(repo, rev) { return None }
        let sql = "SELECT path FROM blob_index
                   WHERE repo = ?1 AND rev = ?2 AND path GLOB ?3";
        let rows = self.conn.prepare_cached(sql)?
            .query_map([repo, rev, pattern.src.as_ref()], |r| r.get::<_, String>(0))?;
        Some(rows.map(|p| FilePath(Arc::from(Path::new(&p)))).collect())
    }

    fn upsert_rev(&self, repo, rev, entries) {
        let tx = self.conn.transaction()?;
        tx.execute("DELETE FROM blob_index WHERE repo=?1 AND rev=?2", [repo, rev])?;
        let mut ins = tx.prepare_cached(
            "INSERT INTO blob_index VALUES (?1,?2,?3,?4)")?;
        for (fp, oid) in entries {
            ins.execute(params![repo, rev, fp.as_str(), &oid[..]])?;
        }
        tx.commit()?;
    }

    fn drop_rev(&self, repo, rev) {
        self.conn.execute("DELETE FROM blob_index WHERE repo=?1 AND rev=?2", [repo, rev])?;
    }

    fn has_rev(&self, repo, rev) -> bool {
        self.conn.query_row(
            "SELECT 1 FROM blob_index WHERE repo=?1 AND rev=?2 LIMIT 1",
            [repo, rev], |_| Ok(true)).unwrap_or(false)
    }
}
```

## Pattern→GLOB translation

- sprefa's `CompiledPattern` is globset-based
- SQLite's `GLOB` supports `*`, `?`, `[abc]` — same subset we accept
- For unsupported edge cases (globset `{a,b}` expansion), fall back to
  fetching all `(repo, rev)` rows and filtering in-process

## NoopStore

```rust
impl PathIndex for NoopStore {
    fn files_at(&self, _, _, _) -> Option<Vec<FilePath>> { None }
    fn upsert_rev(&self, _, _, _) {}
    fn drop_rev(&self, _, _) {}
    fn has_rev(&self, _, _) -> bool { false }
}
```

## Blast radius

- `v2/src/store/_1_path_index.rs` — new trait, ~40 lines
- `v2/src/store/_0_noop.rs` — add empty impl, ~15 lines
- `v2/src/store/_2_sqlite.rs` — new file or extend existing, DDL + impl, ~150 lines
- `v2/src/store/mod.rs` — export trait
- Tests: round-trip insert/query/drop, GLOB behavior, concurrent readers

## Depends on / depended on by

- Depends: `Store` / `SqliteStore` Phase 2 foundation (6c can land a minimal
  SQLite connection if Phase 2 hasn't landed yet)
- Depended on: 6d (reader queries the index), 6f (invalidation calls drop_rev)
