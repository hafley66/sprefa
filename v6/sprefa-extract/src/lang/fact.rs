//! `FactMatcher`: an ast-grep `Matcher` (core matcher.rs:27-48) whose predicate
//! is a row in a dl6 store, so a rule can say "this node names something the
//! database already knows" and compose that with `ops::All`/`Any`/`Not`
//! (core ops.rs:45, 107, 197) beside any pattern or kind matcher.
//!
//! A stored rel keys on INTEGER surrogates and its TEXT columns are `__str`
//! references, so reading a column's values is a dictionary join, never a
//! column read (`.claude/skills/sql-relational-design`). The join runs ONCE per
//! (rel, column) per run into a [`FactSet`]; the per-node predicate is set
//! membership, never a query.
//! @comment-ok: module header, the shape every lang/*.rs opens with

use std::borrow::Cow;
use std::collections::{BTreeMap, BTreeSet};
use std::path::{Path, PathBuf};
use std::sync::Arc;

use ast_grep_core::meta_var::MetaVarEnv;
use ast_grep_core::{Doc, Matcher, Node};
use rusqlite::{Connection, OpenFlags};

/// `~/.agent/dl6.db` relative to `$HOME`. One server, one db: every dl6 program
/// writes here and a reader opens this file, never a copy of it.
pub const DL6_DB_RELATIVE_PATH: &str = ".agent/dl6.db";

#[derive(Clone, Debug, Eq, PartialEq)]
pub enum FactError {
    NoHome,
    Open { path: PathBuf, message: String },
    Query { statement: String, message: String },
    /// A rel or column name that cannot be a SQL identifier here. The names
    /// reach this crate from a program, so the shape is checked, not escaped.
    Name(String),
}

impl std::fmt::Display for FactError {
    fn fmt(&self, formatter: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::NoHome => write!(formatter, "HOME is unset, so {DL6_DB_RELATIVE_PATH} has no path"),
            Self::Open { path, message } => {
                write!(formatter, "open {} read-only: {message}", path.display())
            }
            Self::Query { statement, message } => write!(formatter, "{statement}: {message}"),
            Self::Name(name) => write!(formatter, "not a rel or column name: {name}"),
        }
    }
}

impl std::error::Error for FactError {}

/// The one dl6 store's path.
pub fn dl6_db_path() -> Result<PathBuf, FactError> {
    let home = std::env::var_os("HOME").ok_or(FactError::NoHome)?;
    Ok(PathBuf::from(home).join(DL6_DB_RELATIVE_PATH))
}

/// `SQLITE_OPEN_READ_ONLY` alone still lets SQLite write a WAL database's
/// `-shm`/`-wal` sidecars, so the URI carries `mode=ro` too.
pub fn open_dl6_readonly() -> Result<Connection, FactError> {
    open_readonly(&dl6_db_path()?)
}

/// One read-only connection on any store file.
pub fn open_readonly(path: &Path) -> Result<Connection, FactError> {
    let uri = format!("file:{}?mode=ro", path.display());
    Connection::open_with_flags(
        &uri,
        OpenFlags::SQLITE_OPEN_READ_ONLY | OpenFlags::SQLITE_OPEN_URI,
    )
    .map_err(|error| FactError::Open {
        path: path.to_path_buf(),
        message: error.to_string(),
    })
}

/// The values one (rel, column) holds, read once. `BTreeSet` so `values()` is
/// ordered and a caller's own output does not move with hash seeds.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct FactSet {
    rel: String,
    column: String,
    values: BTreeSet<String>,
}

impl FactSet {
    /// ONE statement per (rel, column) per run: the dictionary join that turns
    /// the column's surrogate ids back into text at the read boundary.
    pub fn load(connection: &Connection, rel: &str, column: &str) -> Result<Self, FactError> {
        let rel = identifier(rel)?;
        let column = identifier(column)?;
        let statement = format!(
            "SELECT DISTINCT s.\"content\" FROM \"{rel}\" t \
             JOIN \"__str\" s ON s.\"__id\" = t.\"{column}\""
        );
        let mut prepared = connection
            .prepare(&statement)
            .map_err(|error| FactError::Query {
                statement: statement.clone(),
                message: error.to_string(),
            })?;
        let values = prepared
            .query_map([], |row| row.get::<_, String>(0))
            .and_then(|rows| rows.collect::<Result<BTreeSet<String>, _>>())
            .map_err(|error| FactError::Query {
                statement: statement.clone(),
                message: error.to_string(),
            })?;
        Ok(Self {
            rel,
            column,
            values,
        })
    }

    /// ONE statement, grouped: the `column` set per distinct `key_column`. How a
    /// per-file question is answered without a query per file.
    pub fn load_by(
        connection: &Connection,
        rel: &str,
        key_column: &str,
        column: &str,
    ) -> Result<BTreeMap<String, Arc<Self>>, FactError> {
        let rel = identifier(rel)?;
        let key_column = identifier(key_column)?;
        let column = identifier(column)?;
        let statement = format!(
            "SELECT DISTINCT k.\"content\", v.\"content\" FROM \"{rel}\" t \
             JOIN \"__str\" k ON k.\"__id\" = t.\"{key_column}\" \
             JOIN \"__str\" v ON v.\"__id\" = t.\"{column}\""
        );
        let query = |error: rusqlite::Error| FactError::Query {
            statement: statement.clone(),
            message: error.to_string(),
        };
        let mut prepared = connection.prepare(&statement).map_err(query)?;
        let pairs = prepared
            .query_map([], |row| {
                Ok((row.get::<_, String>(0)?, row.get::<_, String>(1)?))
            })
            .and_then(|rows| rows.collect::<Result<Vec<(String, String)>, _>>())
            .map_err(query)?;
        let mut by_key: BTreeMap<String, BTreeSet<String>> = BTreeMap::new();
        for (key, value) in pairs {
            by_key.entry(key).or_default().insert(value);
        }
        Ok(by_key
            .into_iter()
            .map(|(key, values)| {
                let set = Self {
                    rel: rel.clone(),
                    column: column.clone(),
                    values,
                };
                (key, Arc::new(set))
            })
            .collect())
    }

    /// The set a caller already holds, for a run with no store behind it.
    pub fn from_values<I, S>(rel: &str, column: &str, values: I) -> Result<Self, FactError>
    where
        I: IntoIterator<Item = S>,
        S: Into<String>,
    {
        Ok(Self {
            rel: identifier(rel)?,
            column: identifier(column)?,
            values: values.into_iter().map(Into::into).collect(),
        })
    }

    pub fn rel(&self) -> &str {
        &self.rel
    }

    pub fn column(&self) -> &str {
        &self.column
    }

    pub fn len(&self) -> usize {
        self.values.len()
    }

    pub fn is_empty(&self) -> bool {
        self.values.is_empty()
    }

    pub fn contains(&self, value: &str) -> bool {
        self.values.contains(value)
    }

    pub fn values(&self) -> impl Iterator<Item = &str> {
        self.values.iter().map(String::as_str)
    }

    /// One matcher over this set. Presence is decided here, so the per-node
    /// predicate stays a string compare.
    pub fn matcher(self: &Arc<Self>, value: impl Into<String>) -> FactMatcher {
        FactMatcher::new(Arc::clone(self), value)
    }
}

/// A node matches when its text equals `value` and `value` is a value of
/// `rel`.`column`. Cheap to clone: the set is shared, never copied.
#[derive(Clone, Debug)]
pub struct FactMatcher {
    set: Arc<FactSet>,
    value: String,
    present: bool,
}

impl FactMatcher {
    pub fn new(set: Arc<FactSet>, value: impl Into<String>) -> Self {
        let value = value.into();
        let present = set.contains(&value);
        Self { set, value, present }
    }

    pub fn rel(&self) -> &str {
        self.set.rel()
    }

    pub fn column(&self) -> &str {
        self.set.column()
    }

    pub fn value(&self) -> &str {
        &self.value
    }

    /// Whether the store carries this value at all. A false here makes the
    /// matcher match nothing, whatever the tree holds.
    pub fn present(&self) -> bool {
        self.present
    }
}

impl Matcher for FactMatcher {
    fn match_node_with_env<'tree, D: Doc>(
        &self,
        node: Node<'tree, D>,
        _env: &mut Cow<MetaVarEnv<'tree, D>>,
    ) -> Option<Node<'tree, D>> {
        (self.present && node.text() == self.value).then_some(node)
    }
}

/// dl6 rel and column names are `[A-Za-z0-9_]`; anything else is rejected
/// rather than quoted, so no caller can reach the SQL through a name.
fn identifier(name: &str) -> Result<String, FactError> {
    let legal = !name.is_empty()
        && name
            .bytes()
            .all(|byte| byte.is_ascii_alphanumeric() || byte == b'_');
    legal
        .then(|| name.to_string())
        .ok_or_else(|| FactError::Name(name.to_string()))
}
