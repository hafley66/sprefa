//! One enum per end, one variant per impl; the `Recipe` picks the variant.

use crate::_0_id::Dict;
use crate::_5_ends::{Change, Delta, Diag, Engine, Store, Table};
use crate::_6_back::_1_dd::Dd;
use crate::_7_store::_1_sqlite::Sqlite;

pub enum AnyEngine {
    Dd(Dd),
}

impl Engine for AnyEngine {
    fn step(&mut self, input: Vec<Change>) -> Result<Delta, Diag> {
        match self {
            AnyEngine::Dd(dd) => dd.step(input),
        }
    }
}

pub enum AnyStore {
    Sqlite(Sqlite),
}

impl Store for AnyStore {
    fn replay(&mut self, dict: &mut Dict) -> Result<Vec<Change>, Diag> {
        match self {
            AnyStore::Sqlite(sqlite) => sqlite.replay(dict),
        }
    }

    fn write(&mut self, delta: &Delta, fresh: &[(u64, &[u8])]) -> Result<usize, Diag> {
        match self {
            AnyStore::Sqlite(sqlite) => sqlite.write(delta, fresh),
        }
    }

    fn query(&self, sql: &str) -> Result<Table, Diag> {
        match self {
            AnyStore::Sqlite(sqlite) => sqlite.query(sql),
        }
    }
}
