use sea_orm::{ConnectOptions, ConnectionTrait, Database, DatabaseBackend, DatabaseConnection, Statement};
use sprefa_store::relstore::RelStore;
use std::path::Path;

pub struct StoreDb {
    pub runtime: tokio::runtime::Runtime,
    pub store: RelStore,
}

impl StoreDb {
    pub fn file(path: &Path) -> Self {
        let runtime = tokio::runtime::Builder::new_current_thread().enable_all().build().unwrap();
        let mut options = ConnectOptions::new(format!("sqlite://{}?mode=rwc", path.display()));
        options.max_connections(1).min_connections(1);
        let store = runtime.block_on(async { RelStore::attach(Database::connect(options).await.unwrap()).await.unwrap() });
        Self { runtime, store }
    }
    pub fn memory() -> Self {
        let runtime = tokio::runtime::Builder::new_current_thread().enable_all().build().unwrap();
        let mut options = ConnectOptions::new("sqlite::memory:");
        options.max_connections(1).min_connections(1);
        let store = runtime.block_on(async { RelStore::attach(Database::connect(options).await.unwrap()).await.unwrap() });
        Self { runtime, store }
    }
    pub fn conn(&self) -> &DatabaseConnection { self.store.conn() }
    pub fn exec(&self, sql: impl AsRef<str>) {
        self.runtime.block_on(self.conn().execute_unprepared(sql.as_ref())).unwrap();
    }
    pub fn rows(&self, sql: impl Into<String>) -> Vec<sea_orm::QueryResult> {
        let statement = Statement::from_string(DatabaseBackend::Sqlite, sql.into());
        self.runtime.block_on(self.conn().query_all_raw(statement)).unwrap()
    }
}
