//! Every SQLite-specific unfuckening, behind a trait so nothing else in the
//! crate touches a raw pragma or a dialect quirk. "A trait over whatever it
//! needs to be a trait" — implemented on `DatabaseBackend`.

use sea_orm::sea_query::{SqliteQueryBuilder, TableCreateStatement};
use sea_orm::DatabaseBackend;

/// Pragmas a fresh store opens with:
///  - `foreign_keys=ON`  — FKs are enforced, not decorative (owner law: wrong
///                          FK usage = unplugged).
///  - `journal_mode=WAL` — concurrent readers while the writer ticks.
///  - `synchronous=NORMAL`, `temp_store=MEMORY` — the ast-grep/biome-speed knobs.
pub const OPEN_PRAGMAS: &str = "PRAGMA journal_mode=WAL;\
PRAGMA synchronous=NORMAL;\
PRAGMA foreign_keys=ON;\
PRAGMA temp_store=MEMORY;";

pub trait UnfuckSqlite {
    /// DDL for a table-create statement, unfucked for SQLite:
    ///
    ///  1. **AUTOINCREMENT stripped.** A plain `INTEGER PRIMARY KEY` is the
    ///     rowid alias — dense, free, no extra B-tree. `AUTOINCREMENT` only adds
    ///     the `sqlite_sequence` bookkeeping table to prevent rowid reuse, which
    ///     we never need. (v5 never hit this because v5 never used a DB-assigned
    ///     id — it hashed content into the id. This is the fix for doing it right.)
    ///  2. **`WITHOUT ROWID` appended** for composite-key junctions. The table
    ///     becomes its own PK index instead of paying for a rowid heap PLUS a
    ///     full-width duplicate autoindex — the 256.6 MB duplicate-copy bucket
    ///     that all-columns PKs cost v5.
    fn ddl_for(&self, stmt: TableCreateStatement, without_rowid: bool) -> String;
}

impl UnfuckSqlite for DatabaseBackend {
    fn ddl_for(&self, stmt: TableCreateStatement, without_rowid: bool) -> String {
        let mut sql = stmt.to_string(SqliteQueryBuilder);
        sql = sql.replace(" AUTOINCREMENT", "");
        if without_rowid {
            sql.push_str(" WITHOUT ROWID");
        }
        sql
    }
}
