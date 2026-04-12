use tower_lsp::lsp_types::{Hover, HoverContents, MarkupContent, MarkupKind};

use crate::db::DbProvider;

/// Provides hover information for capture variables by querying the database.
pub struct HoverProvider {
    db: Option<DbProvider>,
}

impl HoverProvider {
    /// Create a new HoverProvider with an optional database connection.
    pub fn new(db_path: Option<&std::path::Path>) -> Self {
        log::debug!("creating HoverProvider with db_path: {:?}", db_path);
        let db = db_path.and_then(|p| {
            match DbProvider::open(p) {
                Ok(provider) => {
                    log::info!("hover provider: database connected");
                    Some(provider)
                }
                Err(e) => {
                    log::warn!("hover provider: failed to open database: {}", e);
                    None
                }
            }
        });
        Self { db }
    }

    /// Create a new HoverProvider with an existing DbProvider.
    pub fn with_db(db: DbProvider) -> Self {
        Self { db: Some(db) }
    }

    /// Create a new HoverProvider without database access.
    pub fn empty() -> Self {
        log::debug!("creating empty HoverProvider (no database)");
        Self { db: None }
    }

    /// Get hover information for a variable in a rule.
    ///
    /// Returns `None` if no values are found or the database is unavailable.
    pub fn hover(&self, var_name: &str, rule_name: &str, namespace: Option<&str>) -> Option<Hover> {
        log::info!("hover: querying for var='{}' in rule='{}' (namespace={:?})", var_name, rule_name, namespace);
        
        let db = self.db.as_ref()?;

        // Query for values
        log::debug!("hover: executing database query");
        let values = db.query_values(rule_name, var_name, namespace);

        if values.is_empty() {
            log::info!("hover: no values found for ${} in {}", var_name, rule_name);
            return None;
        }

        let total = values.len();
        log::info!("hover: found {} values for ${} in {}", total, var_name, rule_name);

        let mut markdown = format!(
            "**Capture: ${}**\nRule: {}\n\nValues ({} total):\n",
            var_name, rule_name, total
        );

        for value in values {
            markdown.push_str(&format!("- {}\n", value));
        }

        Some(Hover {
            contents: HoverContents::Markup(MarkupContent {
                kind: MarkupKind::Markdown,
                value: markdown,
            }),
            range: None,
        })
    }

    /// Get raw values for a variable from the database.
    /// Returns empty vector if no DB or no values found.
    pub fn get_values(&self, key: &str, namespace: Option<&str>) -> Vec<String> {
        let db = match self.db.as_ref() {
            Some(d) => d,
            None => return Vec::new(),
        };

        // Parse key as "rule.var" or just "var"
        let (rule_name, var_name) = match key.find('.') {
            Some(idx) => (&key[..idx], &key[idx + 1..]),
            None => ("", key),
        };

        db.query_values(rule_name, var_name, namespace)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use tempfile::TempDir;

    fn setup_test_db() -> (DbProvider, TempDir) {
        let tmp = TempDir::new().unwrap();
        let db_path = tmp.path().join("test.db");

        let conn = rusqlite::Connection::open(&db_path).unwrap();

        // Create strings table
        conn.execute(
            "CREATE TABLE strings (id INTEGER PRIMARY KEY, value TEXT)",
            [],
        )
        .unwrap();

        // Insert test strings
        conn.execute("INSERT INTO strings (value) VALUES ('sprefa_sprf')", [])
            .unwrap();
        conn.execute("INSERT INTO strings (value) VALUES ('sprefa_config')", [])
            .unwrap();
        conn.execute("INSERT INTO strings (value) VALUES ('sprefa_scan')", [])
            .unwrap();

        // Create a rule data table
        conn.execute(
            "CREATE TABLE myrule__data (id INTEGER PRIMARY KEY, myvar_ref INTEGER)",
            [],
        )
        .unwrap();
        conn.execute("INSERT INTO myrule__data (myvar_ref) VALUES (1)", [])
            .unwrap();
        conn.execute("INSERT INTO myrule__data (myvar_ref) VALUES (2)", [])
            .unwrap();
        conn.execute("INSERT INTO myrule__data (myvar_ref) VALUES (3)", [])
            .unwrap();

        drop(conn);

        let provider = DbProvider::open(&db_path).unwrap();
        (provider, tmp)
    }

    #[test]
    fn test_hover_with_values() {
        let (db, _tmp) = setup_test_db();
        let provider = HoverProvider::with_db(db);

        let hover = provider.hover("myvar", "myrule", None).expect("should have hover");

        let HoverContents::Markup(content) = hover.contents else {
            panic!("expected markup content");
        };

        assert_eq!(content.kind, MarkupKind::Markdown);
        assert!(content.value.contains("**Capture: $myvar**"));
        assert!(content.value.contains("Rule: myrule"));
        assert!(content.value.contains("Values (3 total):"));
        assert!(content.value.contains("- sprefa_sprf"));
        assert!(content.value.contains("- sprefa_config"));
        assert!(content.value.contains("- sprefa_scan"));
    }

    #[test]
    fn test_hover_no_values() {
        let (db, _tmp) = setup_test_db();
        let provider = HoverProvider::with_db(db);

        // Non-existent variable
        let hover = provider.hover("nonexistent", "myrule", None);
        assert!(hover.is_none());
    }

    #[test]
    fn test_hover_empty_provider() {
        let provider = HoverProvider::empty();

        let hover = provider.hover("myvar", "myrule", None);
        assert!(hover.is_none());
    }

    #[test]
    fn test_hover_new_with_path() {
        let tmp = TempDir::new().unwrap();
        let db_path = tmp.path().join("test.db");

        // Create a valid database
        let conn = rusqlite::Connection::open(&db_path).unwrap();
        conn.execute(
            "CREATE TABLE strings (id INTEGER PRIMARY KEY, value TEXT)",
            [],
        )
        .unwrap();
        conn.execute("INSERT INTO strings (value) VALUES ('test')", [])
            .unwrap();
        conn.execute(
            "CREATE TABLE myrule__data (id INTEGER PRIMARY KEY, myvar_ref INTEGER)",
            [],
        )
        .unwrap();
        conn.execute("INSERT INTO myrule__data (myvar_ref) VALUES (1)", [])
            .unwrap();
        drop(conn);

        let provider = HoverProvider::new(Some(&db_path));
        let hover = provider.hover("myvar", "myrule", None);
        assert!(hover.is_some());
    }

    #[test]
    fn test_hover_new_with_none_path() {
        let provider = HoverProvider::new(None::<&'_ std::path::Path>);
        let hover = provider.hover("myvar", "myrule", None);
        assert!(hover.is_none());
    }
}
