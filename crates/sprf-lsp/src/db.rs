use std::path::Path;

use anyhow::Result;
use rusqlite::Connection;

/// Provides database access for LSP hover values.
pub struct DbProvider {
    conn: Connection,
}

impl DbProvider {
    /// Open a connection to the SQLite database at the given path.
    pub fn open(db_path: &Path) -> Result<Self> {
        log::debug!("opening database: {}", db_path.display());
        let conn = Connection::open(db_path)?;
        log::info!("database opened successfully: {}", db_path.display());
        Ok(Self { conn })
    }

    /// Query string values for a given rule, variable, and optional namespace.
    ///
    /// Returns up to 50 distinct values from the strings table joined against
    /// the rule's data table.
    pub fn query_values(
        &self,
        rule_name: &str,
        var_name: &str,
        namespace: Option<&str>,
    ) -> Vec<String> {
        let table = match namespace {
            Some(ns) => format!("{}__{}__data", ns, rule_name),
            None => format!("{}__data", rule_name),
        };
        let column = format!("{}_ref", var_name.to_lowercase());

        let sql = format!(
            "SELECT s.value FROM strings s JOIN {} d ON s.id = d.{} LIMIT 50",
            table, column
        );

        log::debug!("executing SQL query: {}", sql);

        let mut stmt = match self.conn.prepare(&sql) {
            Ok(stmt) => stmt,
            Err(e) => {
                log::debug!("failed to prepare SQL statement: {}", e);
                return vec![];
            }
        };

        let rows = stmt.query_map([], |row| row.get::<_, String>(0));

        match rows {
            Ok(iter) => {
                let values: Vec<String> = iter.filter_map(|r| r.ok()).collect();
                log::debug!("SQL query returned {} values", values.len());
                values
            }
            Err(e) => {
                log::debug!("SQL query failed: {}", e);
                vec![]
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::fs;
    use tempfile::TempDir;

    fn setup_test_db() -> (DbProvider, TempDir) {
        let tmp = TempDir::new().unwrap();
        let db_path = tmp.path().join("test.db");

        let conn = Connection::open(&db_path).unwrap();

        // Create strings table
        conn.execute(
            "CREATE TABLE strings (id INTEGER PRIMARY KEY, value TEXT)",
            [],
        )
        .unwrap();

        // Insert test strings
        conn.execute("INSERT INTO strings (value) VALUES ('foo')", [])
            .unwrap();
        conn.execute("INSERT INTO strings (value) VALUES ('bar')", [])
            .unwrap();
        conn.execute("INSERT INTO strings (value) VALUES ('baz')", [])
            .unwrap();

        // Create a rule data table without namespace
        conn.execute(
            "CREATE TABLE myrule__data (id INTEGER PRIMARY KEY, myvar_ref INTEGER)",
            [],
        )
        .unwrap();
        conn.execute("INSERT INTO myrule__data (myvar_ref) VALUES (1)", [])
            .unwrap();
        conn.execute("INSERT INTO myrule__data (myvar_ref) VALUES (2)", [])
            .unwrap();

        // Create a rule data table with namespace
        conn.execute(
            "CREATE TABLE ns__myrule__data (id INTEGER PRIMARY KEY, myvar_ref INTEGER)",
            [],
        )
        .unwrap();
        conn.execute("INSERT INTO ns__myrule__data (myvar_ref) VALUES (2)", [])
            .unwrap();
        conn.execute("INSERT INTO ns__myrule__data (myvar_ref) VALUES (3)", [])
            .unwrap();

        drop(conn);

        let provider = DbProvider::open(&db_path).unwrap();
        (provider, tmp)
    }

    #[test]
    fn test_open() {
        let tmp = TempDir::new().unwrap();
        let db_path = tmp.path().join("new.db");
        let result = DbProvider::open(&db_path);
        assert!(result.is_ok());
        assert!(fs::metadata(&db_path).is_ok());
    }

    #[test]
    fn test_query_values_without_namespace() {
        let (provider, _tmp) = setup_test_db();
        let values = provider.query_values("myrule", "myvar", None);
        assert_eq!(values.len(), 2);
        assert!(values.contains(&"foo".to_string()));
        assert!(values.contains(&"bar".to_string()));
    }

    #[test]
    fn test_query_values_with_namespace() {
        let (provider, _tmp) = setup_test_db();
        let values = provider.query_values("myrule", "myvar", Some("ns"));
        assert_eq!(values.len(), 2);
        assert!(values.contains(&"bar".to_string()));
        assert!(values.contains(&"baz".to_string()));
    }

    #[test]
    fn test_query_values_case_insensitive_var() {
        let (provider, _tmp) = setup_test_db();
        // Var name should be lowercased for column lookup
        let values = provider.query_values("myrule", "MYVAR", None);
        assert_eq!(values.len(), 2);
    }

    #[test]
    fn test_query_values_missing_table() {
        let (provider, _tmp) = setup_test_db();
        let values = provider.query_values("nonexistent", "myvar", None);
        assert!(values.is_empty());
    }

    #[test]
    fn test_query_values_limit_50() {
        let tmp = TempDir::new().unwrap();
        let db_path = tmp.path().join("limit_test.db");
        let conn = Connection::open(&db_path).unwrap();

        conn.execute(
            "CREATE TABLE strings (id INTEGER PRIMARY KEY, value TEXT)",
            [],
        )
        .unwrap();

        conn.execute(
            "CREATE TABLE many__data (id INTEGER PRIMARY KEY, var_ref INTEGER)",
            [],
        )
        .unwrap();

        // Insert 100 strings and references
        for i in 0..100 {
            conn.execute(
                &format!("INSERT INTO strings (value) VALUES ('value{}')", i),
                [],
            )
            .unwrap();
            conn.execute(
                &format!("INSERT INTO many__data (var_ref) VALUES ({})", i + 1),
                [],
            )
            .unwrap();
        }

        drop(conn);

        let provider = DbProvider::open(&db_path).unwrap();
        let values = provider.query_values("many", "var", None);
        assert_eq!(values.len(), 50);
    }
}
