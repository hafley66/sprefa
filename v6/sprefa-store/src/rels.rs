//! The OPEN core: mint new rel tables at runtime with the sea-query builder.
//!
//! The nine spine tables are CLOSED — the model. A *rel* is a materialized query
//! result whose table is created on demand; this is where a user program extends
//! storage without touching the model. Every rel table is `WITHOUT ROWID` with a
//! composite PK over its key columns (set semantics, no duplicate autoindex —
//! the all-columns-PK fix). Columns are integers (dense ids into the spine) or
//! text, never a smuggled coordinate.

use sea_orm::sea_query::{Alias, ColumnDef, Index, SqliteQueryBuilder, Table};
use sea_orm::{ConnectionTrait, DatabaseConnection, DbErr};

/// A column in a dynamically-created rel table.
pub enum RelCol {
    /// `NOT NULL` integer — a dense id into the spine, or a small ordinal.
    Int(&'static str),
    /// Nullable integer.
    IntNull(&'static str),
    /// `NOT NULL` text — the rare rel that carries a literal, not an id.
    Text(&'static str),
}

impl RelCol {
    fn name(&self) -> &'static str {
        match self {
            RelCol::Int(n) | RelCol::IntNull(n) | RelCol::Text(n) => n,
        }
    }
}

/// Create rel table `name` with `cols`; the PK is the named `pk` columns and the
/// table is `WITHOUT ROWID`. Idempotent (`IF NOT EXISTS`).
pub async fn create_rel_table(
    db: &DatabaseConnection,
    name: &str,
    cols: &[RelCol],
    pk: &[&str],
) -> Result<(), DbErr> {
    let mut t = Table::create();
    t.table(Alias::new(name)).if_not_exists();
    for c in cols {
        let mut def = ColumnDef::new(Alias::new(c.name()));
        match c {
            RelCol::Int(_) => {
                def.integer().not_null();
            }
            RelCol::IntNull(_) => {
                def.integer();
            }
            RelCol::Text(_) => {
                def.text().not_null();
            }
        }
        t.col(&mut def);
    }
    if !pk.is_empty() {
        let mut idx = Index::create();
        for k in pk {
            idx.col(Alias::new(*k));
        }
        t.primary_key(&mut idx);
    }
    let mut sql = t.to_string(SqliteQueryBuilder);
    if !pk.is_empty() {
        sql.push_str(" WITHOUT ROWID");
    }
    db.execute_unprepared(&sql).await?;
    Ok(())
}

/// Drop a rel table (an evicted / no-longer-subscribed rel leaves zero bytes).
pub async fn drop_rel_table(db: &DatabaseConnection, name: &str) -> Result<(), DbErr> {
    let sql = Table::drop()
        .table(Alias::new(name))
        .if_exists()
        .to_string(SqliteQueryBuilder);
    db.execute_unprepared(&sql).await?;
    Ok(())
}
