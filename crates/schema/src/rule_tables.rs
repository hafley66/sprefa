/// Per-rule SQLite table DDL generation.
///
/// Each extraction rule gets its own table with one row per extraction event.
/// Dual columns per capture: `{name}_ref` (provenance/spans) and `{name}_str`
/// (fast string reads). Plus `repo_id`, `file_id`, `rev` for context.

/// Metadata for one capture column.
pub struct RuleColumn {
    /// Lowercase variable name (e.g. "svc", "repo", "tag").
    pub name: String,
    /// "repo" or "rev" if this capture drives demand scanning.
    pub scan: Option<String>,
}

/// Metadata for a per-rule table.
pub struct RuleTableDef {
    pub rule_name: String,
    pub columns: Vec<RuleColumn>,
}

/// Target for demand scanning, derived from annotated columns.
pub struct ScanTarget {
    pub table: String,
    pub column: String,
    /// "repo" or "rev"
    pub kind: String,
}

/// Paired repo+rev columns from one rule for demand scanning.
#[derive(Debug, Clone)]
pub struct ScanPair {
    pub table: String,
    pub repo_column: String,
    pub rev_column: String,
}

impl RuleTableDef {
    /// The underlying data table name: `{rule_name}_data`.
    pub fn data_table_name(&self) -> String {
        format!("{}_data", self.rule_name)
    }

    /// CREATE TABLE IF NOT EXISTS for this rule's data table.
    pub fn create_table_sql(&self) -> String {
        let mut cols = vec!["id INTEGER PRIMARY KEY".to_string()];
        for c in &self.columns {
            cols.push(format!("{}_ref INTEGER", c.name));
            cols.push(format!("{}_str INTEGER", c.name));
        }
        cols.push("repo_id INTEGER".to_string());
        cols.push("file_id INTEGER".to_string());
        cols.push("rev TEXT".to_string());

        format!(
            "CREATE TABLE IF NOT EXISTS \"{}\" (\n  {}\n)",
            self.data_table_name(),
            cols.join(",\n  ")
        )
    }

    /// CREATE VIEW for fast string-value access (JOIN strings only).
    pub fn create_view_sql(&self) -> String {
        let mut select_cols = vec!["t.id".to_string()];
        let mut joins = Vec::new();

        for (i, c) in self.columns.iter().enumerate() {
            let alias = format!("s{}", i);
            select_cols.push(format!("{alias}.value AS {}", c.name));
            select_cols.push(format!("{alias}.norm AS {}_norm", c.name));
            select_cols.push(format!("{alias}.norm2 AS {}_norm2", c.name));
            joins.push(format!(
                "LEFT JOIN strings {alias} ON t.{}_str = {alias}.id",
                c.name
            ));
        }
        select_cols.push("t.repo_id".to_string());
        select_cols.push("t.file_id".to_string());
        select_cols.push("t.rev".to_string());

        format!(
            "CREATE VIEW IF NOT EXISTS \"{name}\" AS\nSELECT {cols}\nFROM \"{name}_data\" t\n{joins}",
            name = self.rule_name,
            cols = select_cols.join(", "),
            joins = joins.join("\n"),
        )
    }

    /// CREATE VIEW for provenance access (JOIN strings + refs for spans/node_path).
    pub fn create_refs_view_sql(&self) -> String {
        let mut select_cols = vec!["t.id".to_string()];
        let mut joins = Vec::new();

        for (i, c) in self.columns.iter().enumerate() {
            let sa = format!("s{}", i);
            let ra = format!("r{}", i);
            select_cols.push(format!("{sa}.value AS {}", c.name));
            select_cols.push(format!("{sa}.norm AS {}_norm", c.name));
            select_cols.push(format!("{sa}.norm2 AS {}_norm2", c.name));
            select_cols.push(format!("{ra}.span_start AS {}_span_start", c.name));
            select_cols.push(format!("{ra}.span_end AS {}_span_end", c.name));
            select_cols.push(format!("{ra}.node_path AS {}_node_path", c.name));
            joins.push(format!(
                "LEFT JOIN strings {sa} ON t.{col}_str = {sa}.id\nLEFT JOIN refs {ra} ON t.{col}_ref = {ra}.id",
                col = c.name,
            ));
        }
        select_cols.push("t.repo_id".to_string());
        select_cols.push("t.file_id".to_string());
        select_cols.push("t.rev".to_string());

        format!(
            "CREATE VIEW IF NOT EXISTS \"{name}_refs\" AS\nSELECT {cols}\nFROM \"{name}_data\" t\n{joins}",
            name = self.rule_name,
            cols = select_cols.join(", "),
            joins = joins.join("\n"),
        )
    }

    /// Scan targets from annotated columns.
    pub fn scan_targets(&self) -> Vec<ScanTarget> {
        self.columns
            .iter()
            .filter_map(|c| {
                c.scan.as_ref().map(|kind| ScanTarget {
                    table: self.rule_name.clone(),
                    column: c.name.clone(),
                    kind: kind.clone(),
                })
            })
            .collect()
    }

    /// Paired scan targets: (repo_column, rev_column) from the same rule.
    /// Returns None if the rule doesn't have both repo and rev annotations.
    pub fn scan_pair(&self) -> Option<ScanPair> {
        let repo_col = self
            .columns
            .iter()
            .find(|c| c.scan.as_deref() == Some("repo"))?;
        let rev_col = self
            .columns
            .iter()
            .find(|c| c.scan.as_deref() == Some("rev"))?;
        Some(ScanPair {
            table: self.rule_name.clone(),
            repo_column: repo_col.name.clone(),
            rev_column: rev_col.name.clone(),
        })
    }

    /// Single-column def for built-in extractors (JS/RS kinds).
    pub fn builtin(rule_name: &str) -> Self {
        RuleTableDef {
            rule_name: rule_name.to_string(),
            columns: vec![RuleColumn {
                name: "value".to_string(),
                scan: None,
            }],
        }
    }

    /// Build from a rule's create_matches definitions.
    pub fn from_matches(rule_name: &str, matches: &[(String, Option<String>)]) -> Self {
        RuleTableDef {
            rule_name: rule_name.to_string(),
            columns: matches
                .iter()
                .map(|(kind, scan)| RuleColumn {
                    name: kind.to_lowercase(),
                    scan: scan.clone(),
                })
                .collect(),
        }
    }
}

/// The 10 built-in kind tables for JS/RS extractors.
pub const BUILTIN_KINDS: &[&str] = &[
    "import_path",
    "import_name",
    "import_alias",
    "export_name",
    "export_local_binding",
    "dep_name",
    "dep_version",
    "rs_use",
    "rs_declare",
    "rs_mod",
];

/// Table definitions for all built-in extractor kinds.
/// Each gets a single "value" column (no scan annotation).
pub fn builtin_rule_table_defs() -> Vec<RuleTableDef> {
    BUILTIN_KINDS.iter().map(|k| RuleTableDef::builtin(k)).collect()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn create_view() {
        let def = RuleTableDef::from_matches(
            "deploy_config",
            &[("SVC".into(), None), ("REPO".into(), Some("repo".into()))],
        );
        let sql = def.create_view_sql();
        assert!(sql.contains("CREATE VIEW IF NOT EXISTS \"deploy_config\""));
        assert!(sql.contains("s0.value AS svc"));
        assert!(sql.contains("s0.norm AS svc_norm"));
        assert!(sql.contains("s0.norm2 AS svc_norm2"));
        assert!(sql.contains("s1.value AS repo"));
        assert!(sql.contains("s1.norm AS repo_norm"));
        assert!(sql.contains("s1.norm2 AS repo_norm2"));
        assert!(sql.contains("FROM \"deploy_config_data\""));
    }

    #[test]
    fn scan_targets() {
        let def = RuleTableDef::from_matches(
            "deploy_config",
            &[
                ("SVC".into(), None),
                ("REPO".into(), Some("repo".into())),
                ("TAG".into(), Some("rev".into())),
            ],
        );
        let targets = def.scan_targets();
        assert_eq!(targets.len(), 2);
        assert_eq!(targets[0].column, "repo");
        assert_eq!(targets[0].kind, "repo");
        assert_eq!(targets[1].column, "tag");
        assert_eq!(targets[1].kind, "rev");
    }
}
