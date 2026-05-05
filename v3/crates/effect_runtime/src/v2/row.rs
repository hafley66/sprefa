//! `Row` — column-addressable carrier abstraction.
//!
//! Lets `FactStore<R>` live in this crate without depending on the
//! consumer's concrete cursor type. `Row: Next`, plus a string-keyed
//! getter and a fields enumeration for serialization.
//!
//! `fields()` returns `Vec<(&str, &str)>` (owned vec, borrowed slices)
//! to keep the trait object-safe and free of associated lifetime
//! parameters. SqliteFactStore consumes it twice per insert (intern +
//! bind) — both inside one borrow window.

use super::next::Next;

pub trait Row: Next + Default {
    /// Read a column by name. None when the column is not present.
    fn get(&self, col: &str) -> Option<&str>;

    /// Set a column. Used by SqliteFactStore when reconstructing rows
    /// from a query result.
    fn set(&mut self, col: &str, value: &str);

    /// Enumerate (col, value) pairs. Order is implementation-defined.
    /// Provided for serialization paths and debugging; SqliteFactStore
    /// drives binding via `get(col)` against its own schema, so the
    /// ordering here does not affect writes.
    fn fields(&self) -> Vec<(&str, &str)>;
}
