//! boop as a library: the relational store over `~/.agent/boop.db` plus the
//! harness adapters that fill it. Linkable from a Rust host (the tauri side of
//! instant) so a caller runs the queries in-process instead of shelling out.
//!
//! The four reads a host needs are `Store::query_status`, `Store::query_facts`,
//! `Store::query_sessions` and `Store::usage_report`; `plans/boop-instant-v2-contract.md`
//! pins the exact call per view.

pub mod bus;
pub mod chat;
pub mod event;
pub mod harness;
pub mod ident;
pub mod identity;
pub mod lane;
pub mod proc;
pub mod query;
pub mod registry;
pub mod rows;
pub mod tail;
pub mod tmux;
pub mod usage;
pub mod worktree;

pub use ident::{Store, SyncStat};
pub use query::{FactKind, FactQuery};
pub use registry::Registry;
pub use rows::{
    CommandRow, EdgeRow, FactCursor, FetchRow, LiveSpanRow, SessionRow, StatusRow, TouchRow,
    TurnRow, UsageRow,
};
pub use usage::{GroupBy, UsageQuery};

/// Open the default store at `~/.agent/boop.db`.
pub fn open_default() -> anyhow::Result<Store> {
    Store::open(Store::default_path()?)
}
