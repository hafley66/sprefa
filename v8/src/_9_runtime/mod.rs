//! The closure at rest. One SQLite file per resident runtime, one table prefix
//! per program name.

#[path = "_1_sqlite.rs"]
pub mod sqlite;
#[path = "_0_store.rs"]
pub mod store;

pub use sqlite::SqliteRowStore;
pub use store::{CellKind, IRowStore, StoreError, Watermark};
