//! Append-only bitemporal fact storage backed by SQLite.

use sea_orm::{
    ConnectionTrait, DatabaseBackend, DatabaseConnection, DbErr, Statement, TransactionTrait,
};
use std::sync::atomic::{AtomicI64, Ordering};

use crate::stmt_counter;

const SOFT_HEAP_LIMIT: &str = "PRAGMA soft_heap_limit=4294967296;";

fn mix_key(key: i64) -> i64 {
    let mut mixed = (key as u64).wrapping_add(0x9E3779B97F4A7C15);
    mixed = (mixed ^ (mixed >> 30)).wrapping_mul(0xBF58476D1CE4E5B9);
    mixed = (mixed ^ (mixed >> 27)).wrapping_mul(0x94D049BB133111EB);
    (mixed ^ (mixed >> 31)) as i64
}

async fn execute(db: &impl ConnectionTrait, sql: &str) -> Result<(), DbErr> {
    stmt_counter::incr();
    db.execute_unprepared(sql).await?;
    Ok(())
}

async fn execute_statement(db: &impl ConnectionTrait, statement: Statement) -> Result<(), DbErr> {
    stmt_counter::incr();
    db.execute_raw(statement).await?;
    Ok(())
}

async fn scalar(db: &impl ConnectionTrait, sql: &str) -> Result<i64, DbErr> {
    stmt_counter::incr();
    Ok(db
        .query_one_raw(Statement::from_string(DatabaseBackend::Sqlite, sql.to_owned()))
        .await?
        .map(|row| row.try_get_by_index::<i64>(0).unwrap_or(0))
        .unwrap_or(0))
}

async fn create_schema(db: &DatabaseConnection) -> Result<(), DbErr> {
    execute(
        db,
        "CREATE TABLE fact(key INTEGER NOT NULL, tt_from INTEGER NOT NULL,
            tt_to INTEGER, weight INTEGER NOT NULL, PRIMARY KEY(key,tt_from)) WITHOUT ROWID;
         CREATE INDEX ix_live ON fact(key) WHERE tt_to IS NULL;
         CREATE TEMP TABLE d(key INTEGER PRIMARY KEY, dw INTEGER);",
    )
    .await
}

fn delta_json(deltas: &[(i64, i64)]) -> String {
    let mut json = String::from("[");
    for (index, (key, weight)) in deltas.iter().enumerate() {
        if index > 0 {
            json.push(',');
        }
        json.push_str(&format!("[{key},{weight}]"));
    }
    json.push(']');
    json
}

pub struct TemporalStore {
    db: DatabaseConnection,
    revision: AtomicI64,
}

impl TemporalStore {
    pub async fn attach(db: DatabaseConnection) -> Result<Self, DbErr> {
        execute(&db, crate::unfuck_sqlite::OPEN_PRAGMAS).await?;
        execute(&db, SOFT_HEAP_LIMIT).await?;
        create_schema(&db).await?;
        let revision = scalar(&db, "SELECT COALESCE(MAX(tt_from), 0) FROM fact").await?;
        Ok(Self {
            db,
            revision: AtomicI64::new(revision),
        })
    }

    pub async fn commit(&self, deltas: &[(i64, i64)]) -> Result<(), DbErr> {
        if deltas.is_empty() {
            return Ok(());
        }
        let revision = self.revision.fetch_add(1, Ordering::Relaxed) + 1;
        let delta_json = delta_json(deltas);
        let transaction = self.db.begin().await?;
        execute(&transaction, "DELETE FROM d").await?;
        execute_statement(
            &transaction,
            Statement::from_sql_and_values(
                DatabaseBackend::Sqlite,
                "INSERT INTO d(key,dw) SELECT json_extract(value,'$[0]'), sum(json_extract(value,'$[1]'))
             FROM json_each(?1) GROUP BY 1",
                [delta_json.into()],
            ),
        )
        .await?;
        execute_statement(
            &transaction,
            Statement::from_sql_and_values(
                DatabaseBackend::Sqlite,
                "INSERT INTO fact(key,tt_from,tt_to,weight)
             SELECT d.key, ?1, NULL, 0 FROM d LEFT JOIN fact f ON f.key=d.key AND f.tt_to IS NULL
             WHERE f.key IS NULL AND d.dw>0",
                [revision.into()],
            ),
        )
        .await?;
        execute(
            &transaction,
            "UPDATE fact SET weight = weight + (SELECT dw FROM d WHERE d.key=fact.key)
             WHERE tt_to IS NULL AND key IN (SELECT key FROM d)",
        )
        .await?;
        execute_statement(
            &transaction,
            Statement::from_sql_and_values(
                DatabaseBackend::Sqlite,
                "UPDATE fact SET tt_to=?1 WHERE tt_to IS NULL AND weight<=0 AND key IN (SELECT key FROM d)",
                [revision.into()],
            ),
        )
        .await?;
        transaction.commit().await?;
        Ok(())
    }

    pub async fn live(&self) -> Result<i64, DbErr> {
        scalar(&self.db, "SELECT count(*) FROM fact WHERE tt_to IS NULL").await
    }

    pub async fn total_rows(&self) -> Result<i64, DbErr> {
        scalar(&self.db, "SELECT count(*) FROM fact").await
    }

    pub async fn digest(&self) -> Result<i64, DbErr> {
        stmt_counter::incr();
        let rows = self
            .db
            .query_all_raw(Statement::from_string(
                DatabaseBackend::Sqlite,
                "SELECT key FROM fact WHERE tt_to IS NULL".to_owned(),
            ))
            .await?;
        Ok(rows.into_iter().fold(0, |digest, row| {
            digest ^ mix_key(row.try_get_by_index::<i64>(0).unwrap_or(0))
        }))
    }

    pub fn conn(&self) -> &DatabaseConnection {
        &self.db
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use sea_orm::{ConnectOptions, Database};

    async fn open() -> TemporalStore {
        static NEXT_TEST: std::sync::atomic::AtomicU64 = std::sync::atomic::AtomicU64::new(0);
        let unique = NEXT_TEST.fetch_add(1, Ordering::Relaxed);
        let path = std::env::temp_dir().join(format!(
            "temporal_store_test_{}_{unique}.sqlite",
            std::process::id()
        ));
        let _ = std::fs::remove_file(&path);
        let mut options = ConnectOptions::new(format!("sqlite://{}?mode=rwc", path.display()));
        options.max_connections(1).min_connections(1);
        TemporalStore::attach(Database::connect(options).await.unwrap())
            .await
            .unwrap()
    }

    fn statement_lock() -> &'static std::sync::Mutex<()> {
        static LOCK: std::sync::Mutex<()> = std::sync::Mutex::new(());
        &LOCK
    }

    #[tokio::test]
    async fn commit_opens_live_intervals_for_a_batch() {
        let _statement_guard = statement_lock().lock().unwrap();
        let store = open().await;
        let deltas = [(10, 1), (20, 1), (30, 1)];
        store.commit(&deltas).await.unwrap();
        assert_eq!(store.live().await.unwrap(), deltas.len() as i64);
        assert_eq!(store.total_rows().await.unwrap(), deltas.len() as i64);
    }

    #[tokio::test]
    async fn retract_closes_the_live_interval_and_keeps_history() {
        let _statement_guard = statement_lock().lock().unwrap();
        let store = open().await;
        store.commit(&[(10, 1), (20, 1)]).await.unwrap();
        store.commit(&[(10, -1)]).await.unwrap();
        assert_eq!(store.live().await.unwrap(), 1);
        assert_eq!(store.total_rows().await.unwrap(), 2);
    }

    #[tokio::test]
    async fn readd_opens_a_new_interval_after_a_close() {
        let _statement_guard = statement_lock().lock().unwrap();
        let store = open().await;
        store.commit(&[(10, 1)]).await.unwrap();
        store.commit(&[(10, -1)]).await.unwrap();
        store.commit(&[(10, 1)]).await.unwrap();
        assert_eq!(store.live().await.unwrap(), 1);
        assert_eq!(store.total_rows().await.unwrap(), 2);
    }

    #[tokio::test]
    async fn commit_statement_count_is_constant_for_a_delta_batch() {
        let _statement_guard = statement_lock().lock().unwrap();
        let store = open().await;
        let deltas: Vec<(i64, i64)> = (0..1_000).map(|key| (key, 1)).collect();
        stmt_counter::reset();
        store.commit(&deltas).await.unwrap();
        assert_eq!(stmt_counter::get(), 5);
    }

    #[tokio::test]
    async fn digest_matches_the_expected_live_key_mix() {
        let _statement_guard = statement_lock().lock().unwrap();
        let store = open().await;
        store.commit(&[(10, 1), (20, 1), (30, 1)]).await.unwrap();
        store.commit(&[(20, -1), (40, 1)]).await.unwrap();
        let expected = [10, 30, 40]
            .into_iter()
            .fold(0, |digest, key| digest ^ mix_key(key));
        assert_eq!(store.digest().await.unwrap(), expected);
    }
}
