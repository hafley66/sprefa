//! SQLite input log (Strategy 4 from chat_log/20260501.1).
//!
//! Stub for card A. Implementation lands in card E (scope construction +
//! cold-start replay). The schema sketched below is the contract that
//! card E must satisfy.
//!
//! Schema (proposed):
//!
//! ```sql
//! CREATE TABLE input_log (
//!     fact      TEXT NOT NULL,
//!     row_hash  BLOB NOT NULL,         -- blake3 of canonicalized row
//!     row_blob  BLOB NOT NULL,         -- bincode-serialized row
//!     gen       INTEGER NOT NULL,
//!     diff      INTEGER NOT NULL,
//!     PRIMARY KEY (fact, row_hash, gen, diff)
//! );
//! CREATE INDEX idx_input_log_gen ON input_log (gen);
//!
//! CREATE TABLE content_tier (
//!     content_hash BLOB PRIMARY KEY,
//!     bytes        BLOB NOT NULL
//! );
//! ```
//!
//! Card E adds: write-through on every push, cold-start scan that calls
//! `runner.push` for every (fact, row, gen, diff) ascending by gen, then
//! advances to the highest gen + 1 and flushes.

use super::_0_row::{Diff, FactName, Row, Time};

/// Placeholder. Constructor + ops will arrive with card E.
pub struct DdPersist;

impl DdPersist {
    pub fn write_input_log(&self, _fact: &FactName, _row: &Row, _t: Time, _diff: Diff) {
        // card E
    }
}
