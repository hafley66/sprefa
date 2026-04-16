# 2a — sqlite foundation

Re-home from v1 into `v2/src/store/`. Pure plumbing: migrations, UDFs,
DDL builders. No `SqliteStore` body yet — that is 2b.

## Prereqs

Phase 1 landed (`store/` module exists, `Store` trait compiles).

## Scope

```
v2/Cargo.toml                    sqlx, blake3 deps
v2/src/store/mod.rs              pub mod _2_sqlite.._5_migrations  (add _2..=_5)
v2/src/store/_3_ddl.rs           NEW  (~200 LOC)
v2/src/store/_4_udfs.rs          NEW  (~120 LOC)
v2/src/store/_5_migrations.rs    NEW  (~80 LOC)
v2/src/store/sql/*.sql           NEW  9 files include_str!'d from _5
v2/src/store/_2_sqlite.rs        NEW but stub; real body in 2b
```

## Files

### v2/Cargo.toml

Under `[dependencies]`:
```
sqlx   = { workspace = true }        # 0.8, features = ["runtime-tokio", "sqlite"]
blake3 = "1"
chrono = { workspace = true }         # already present from Phase 1
```

### v2/src/store/mod.rs

```rust
pub mod _0_types;
pub mod _1_trait;
pub mod _2_sqlite;
pub mod _3_ddl;
pub mod _4_udfs;
pub mod _5_migrations;

pub use _0_types::*;
pub use _1_trait::*;
pub use _2_sqlite::SqliteStore;
```

### v2/src/store/_2_sqlite.rs (stub only in 2a)

```rust
use sqlx::SqlitePool;
use std::{path::Path, sync::Arc};
use tokio::sync::RwLock;
use std::collections::HashMap;
use super::{_1_trait::ExprTableSpec, _0_types::StoreErr};

pub struct SqliteStore {
    pub pool:  SqlitePool,
    pub specs: RwLock<HashMap<Arc<str>, ExprTableSpec>>,
}

impl SqliteStore {
    pub async fn open(path: &Path) -> Result<Arc<Self>, StoreErr> {
        let pool = super::_5_migrations::init_db(path).await?;
        super::_4_udfs::register_all(&pool).await?;
        super::_5_migrations::run_migrations(&pool).await?;
        Ok(Arc::new(Self { pool, specs: RwLock::new(HashMap::new()) }))
    }

    pub async fn open_memory() -> Result<Arc<Self>, StoreErr> {
        Self::open(Path::new(":memory:")).await
    }
}

// impl Store for SqliteStore — lands in 2b
```

### v2/src/store/_3_ddl.rs

Clone Z3 `src/store/_3_ddl.rs` verbatim:
- `data_table_name`, `view_name`, `refs_view_name` (string builders)
- `build_data_table_ddl`, `build_view_ddl`, `build_refs_view_ddl`
- `schema_hash_of(spec)` — blake3 over expr_name + per-capture name + scan_pointer
- `extract_hash_of(expr)` — blake3 over every op's `parse_site().paren_src_bytes()`

V1 source for the table-shape reference: `crates/schema/src/rule_tables.rs:29-90`.
Column set matches v1 one-for-one. Scan-pointer extension adds
`{col}_repo_id` / `{col}_rev_id` / `{col}_file_id` based on
`CaptureColumn::scan_pointer`.

### v2/src/store/_4_udfs.rs

Four scalar UDFs, registered via `conn.lock_handle()` (sqlx 0.8 exposes
`LockedSqliteHandle` → `rusqlite`-compatible handle).

```rust
pub async fn register_all(pool: &SqlitePool) -> Result<(), StoreErr> {
    let mut conn = pool.acquire().await.map_err(|e| StoreErr::Sql(e.to_string()))?;
    let mut handle = conn.lock_handle().await.map_err(|e| StoreErr::Sql(e.to_string()))?;
    let flags = rusqlite::functions::FunctionFlags::SQLITE_UTF8
              | rusqlite::functions::FunctionFlags::SQLITE_DETERMINISTIC;
    handle.create_scalar_function("sprf_norm",  1, flags, sprf_norm)?;
    handle.create_scalar_function("fzy_score",  2, flags, fzy_score)?;
    handle.create_scalar_function("re_extract", 2, flags.difference(rusqlite::functions::FunctionFlags::SQLITE_DETERMINISTIC), re_extract)?;
    handle.create_scalar_function("split_part", 3, flags, split_part)?;
    Ok(())
}
```

Body of each UDF: clone verbatim from `crates/schema/src/udfs.rs` (504
LOC, all four impls already debugged). Copy the unit tests too into a
`#[cfg(test)]` block at bottom.

### v2/src/store/_5_migrations.rs

Z3 pseudo. Uses `SqliteConnectOptions` with WAL / foreign_keys /
synchronous NORMAL. `max_connections(8)`.

SQL files in `v2/src/store/sql/`:
```
repos.sql          files.sql          strings_fts.sql
refs.sql           rev_files.sql      repo_revs.sql
sprf_meta.sql      mutations.sql      discovery_log.sql
```

All except `mutations.sql` clone byte-for-byte from
`crates/schema/src/migrations.rs` (219 LOC, one `CREATE TABLE` block
each). `mutations.sql` is new (Z3 pseudo specifies the DDL at
lines 987–999 of the design doc).

## Z3 deviations

- **UDF registration path**: Z3 pseudo gestures at raw FFI
  (`rusqlite_ish_ffi::*`). Reality in sqlx 0.8 is `conn.lock_handle()`
  which returns a `LockedSqliteHandle` with
  `create_scalar_function` via the bundled rusqlite handle. Plumbing
  delta absorbed inside `_4_udfs.rs`.
- **SQL files**: Z3 inlines `mutations.sql` as a comment at
  lines 987-999. Land it as a real file for symmetry with the v1 clones.

## Verify

```
cd v2 && cargo build --lib 2>&1 | tail -20
```

Expect warnings (unused `SqliteStore` — fine, 2b uses it).

Smoke-test in a throwaway test file:
```rust
#[tokio::test]
async fn smoke_sqlite_open_and_migrate() {
    let store = v2::store::SqliteStore::open_memory().await.unwrap();
    let pool = &store.pool;
    let tables: Vec<(String,)> = sqlx::query_as("SELECT name FROM sqlite_master WHERE type='table'")
        .fetch_all(pool).await.unwrap();
    let names: std::collections::HashSet<_> = tables.iter().map(|(n,)| n.as_str()).collect();
    for t in ["repos","files","strings","refs","rev_files","repo_revs","sprf_meta","mutations","discovery_log"] {
        assert!(names.contains(t), "missing table: {t}");
    }
}
```

## Exit state

- `cargo build --lib` green, warnings OK
- Smoke test passes: open_memory → all 9 tables exist → 4 UDFs callable
- `SqliteStore::open` + `open_memory` exist, no `Store` impl yet
