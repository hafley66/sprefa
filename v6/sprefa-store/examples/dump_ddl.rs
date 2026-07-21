//! Prints the schema the crate GENERATES from the entities — the proof that the
//! DDL is derived (single source of truth), FKs are real, junctions are WITHOUT
//! ROWID, and no AUTOINCREMENT survives. `cargo run --example dump_ddl`.

use sea_orm::{ConnectionTrait, DatabaseBackend, Statement};
use sprefa_store::Store;

#[tokio::main]
async fn main() {
    let store = Store::open("sqlite::memory:").await.unwrap();
    let rows = store
        .db()
        .query_all_raw(Statement::from_string(
            DatabaseBackend::Sqlite,
            "SELECT sql FROM sqlite_master WHERE sql IS NOT NULL ORDER BY type DESC, name"
                .to_owned(),
        ))
        .await
        .unwrap();
    for r in rows {
        let sql: String = r.try_get_by_index(0).unwrap();
        println!("{sql};\n");
    }
}
