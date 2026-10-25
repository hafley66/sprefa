//! The store's SQL text, built once from a `Plan`. Every row-moving statement takes its whole
//! batch as one JSON array `?1`.

use crate::_2_ir::{Col, RelKind};
use crate::_2a_diag::Diag;
use crate::_3_plan::{Plan, RelId};
use std::collections::HashSet;

/// `dict` and `meta`; created before `meta` is checked.
pub const FIXED: &str = "
CREATE TABLE IF NOT EXISTS dict (id INTEGER PRIMARY KEY, bytes BLOB NOT NULL);
CREATE TABLE IF NOT EXISTS meta (recipe TEXT NOT NULL, plan_hash INTEGER NOT NULL);
";

/// `?1` is `[[id, hex bytes], ...]`.
pub const DICT_INSERT: &str = "INSERT INTO dict (id, bytes)
  SELECT value->>0, unhex(value->>1) FROM json_each(?1) WHERE true ON CONFLICT (id) DO NOTHING";

/// A nullary rel still needs one column; its one row is `unit = 0`.
const UNIT: &str = "unit";

/// One rel's statements. `?1` of `insert` and `delete` is `[[col0, col1, ...], ...]`.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct RelSql {
    pub rel: RelId,
    pub kind: RelKind,
    /// Cols of the rel; a stored nullary row has the one `unit` column and reads back as `[]`.
    pub width: usize,
    pub create: String,
    pub insert: String,
    pub delete: String,
    pub select: String,
}

/// `"name"` with inner quotes doubled.
pub fn quote(name: &str) -> String {
    format!("\"{}\"", name.replace('"', "\"\""))
}

/// Every rel's statements, in `RelId` order. A table or view name equal to another, `dict`,
/// `meta` or `sqlite_*` under ASCII case folding is a `Diag::Store`.
pub fn rel_sql(plan: &Plan) -> Result<Vec<RelSql>, Diag> {
    let mut taken: HashSet<String> = ["dict", "meta"].map(String::from).into();
    for rel in &plan.rels {
        for name in [rel.name.clone(), format!("{}_text", rel.name)] {
            let folded = name.to_ascii_lowercase();
            if folded.starts_with("sqlite_") || !taken.insert(folded) {
                return Err(Diag::Store(format!(
                    "table or view name {name:?} collides with another store name"
                )));
            }
        }
    }
    Ok(plan
        .program
        .rels
        .iter()
        .zip(&plan.rels)
        .enumerate()
        .map(|(index, (written, rel))| {
            let table = quote(&rel.name);
            let names: Vec<String> = if written.cols.is_empty() {
                vec![UNIT.to_string()]
            } else {
                written.cols.iter().map(|(name, _)| quote(name)).collect()
            };
            let list = names.join(", ");
            let values = if written.cols.is_empty() {
                "0".to_string()
            } else {
                (0..names.len())
                    .map(|slot| format!("value->>{slot}"))
                    .collect::<Vec<_>>()
                    .join(", ")
            };
            let mut defs: Vec<String> = names
                .iter()
                .map(|name| format!("{name} INTEGER NOT NULL"))
                .collect();
            if written.cols.is_empty() {
                defs[0].push_str(" CHECK (unit = 0)");
            }
            let decoded = if written.cols.is_empty() {
                format!("r.{UNIT} AS {UNIT}")
            } else {
                written
                    .cols
                    .iter()
                    .enumerate()
                    .map(|(slot, (name, col))| match col {
                        Col::Text => format!("CAST(d{slot}.bytes AS TEXT) AS {}", quote(name)),
                        Col::Int => format!("r.{0} AS {0}", quote(name)),
                    })
                    .collect::<Vec<_>>()
                    .join(", ")
            };
            let joins: String = written
                .cols
                .iter()
                .enumerate()
                .filter(|(_, (_, col))| *col == Col::Text)
                .map(|(slot, (name, _))| {
                    format!(
                        " LEFT JOIN dict AS d{slot} ON d{slot}.id = r.{}",
                        quote(name)
                    )
                })
                .collect();
            let view = quote(&format!("{}_text", rel.name));
            RelSql {
                rel: index as RelId,
                kind: rel.kind.clone(),
                width: written.cols.len(),
                create: format!(
                    "CREATE TABLE IF NOT EXISTS {table} ({}, PRIMARY KEY ({list})) WITHOUT ROWID;\n\
                     CREATE VIEW IF NOT EXISTS {view} AS SELECT {decoded} FROM {table} AS r{joins};\n",
                    defs.join(", ")
                ),
                insert: format!("INSERT INTO {table} ({list}) SELECT {values} FROM json_each(?1)"),
                delete: format!(
                    "DELETE FROM {table} WHERE ({list}) IN (SELECT {values} FROM json_each(?1))"
                ),
                select: format!("SELECT {list} FROM {table}"),
            }
        })
        .collect())
}
