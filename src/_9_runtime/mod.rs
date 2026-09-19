//! The closure at rest. One SQLite file per resident runtime, one table prefix
//! per program name.

#[path = "_3_executors/mod.rs"]
pub mod executors;
#[path = "_2_reconcile.rs"]
pub mod reconcile;
#[path = "_1_sqlite.rs"]
pub mod sqlite;
#[path = "_0_store.rs"]
pub mod store;

pub use reconcile::{Answerer, Cadence, IExecutor, Reconciled, Reconciler};
pub use sqlite::{open, sql, SqliteRowStore};
pub use store::{CellKind, IRowStore, StoreError, Watermark};
