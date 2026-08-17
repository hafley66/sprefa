use crate::types::{Arrival, ArrivalSign, RowColumnType, TickDeltas, Value};

pub const REPO_COLUMNS: &[&str] = &["root"];
pub const REV_COLUMNS: &[&str] = &["repo", "oid"];
pub const BLOB_COLUMNS: &[&str] = &["oid"];
pub const FILE_COLUMNS: &[&str] = &["rev", "path", "blob"];
pub const SPAN_COLUMNS: &[&str] = &["file", "start_byte", "end_byte"];
pub const SPECIFIER_COLUMNS: &[&str] = &["owner", "module", "name", "kind"];

/// Relation names from the authored `source.dl6` schema plus the consumer's
/// `source_specifier` extraction projection. Values for `file`, `span`, and
/// `specifier` carry JSON struct values at the runtime boundary; `GenProgram`
/// interns those to SQLite reference IDs through its existing struct plane.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct SourceBindRelations {
    pub repo: String,
    pub rev: String,
    pub blob: String,
    pub file: String,
    pub span: String,
    pub specifier: String,
}

impl Default for SourceBindRelations {
    fn default() -> Self {
        Self {
            repo: "repo".to_string(),
            rev: "rev".to_string(),
            blob: "blob".to_string(),
            file: "file".to_string(),
            span: "span".to_string(),
            specifier: "source_specifier".to_string(),
        }
    }
}

impl SourceBindRelations {
    /// Every source struct declaration. The bind sends arrivals only to
    /// `file`, `span`, and `specifier`; the program's struct plane creates the
    /// enclosing repo/rev/blob rows from the nested values.
    pub fn declarations(&self) -> [SourceBindRelation; 6] {
        [
            SourceBindRelation {
                name: self.repo.clone(),
                columns: REPO_COLUMNS,
                column_types: &[RowColumnType::Text],
                accepts_arrivals: false,
            },
            SourceBindRelation {
                name: self.rev.clone(),
                columns: REV_COLUMNS,
                column_types: &[RowColumnType::Ref, RowColumnType::Text],
                accepts_arrivals: false,
            },
            SourceBindRelation {
                name: self.blob.clone(),
                columns: BLOB_COLUMNS,
                column_types: &[RowColumnType::Text],
                accepts_arrivals: false,
            },
            SourceBindRelation {
                name: self.file.clone(),
                columns: FILE_COLUMNS,
                column_types: &[RowColumnType::Ref, RowColumnType::Text, RowColumnType::Ref],
                accepts_arrivals: true,
            },
            SourceBindRelation {
                name: self.span.clone(),
                columns: SPAN_COLUMNS,
                column_types: &[RowColumnType::Ref, RowColumnType::Int, RowColumnType::Int],
                accepts_arrivals: true,
            },
            SourceBindRelation {
                name: self.specifier.clone(),
                columns: SPECIFIER_COLUMNS,
                column_types: &[
                    RowColumnType::Ref,
                    RowColumnType::Text,
                    RowColumnType::Text,
                    RowColumnType::Text,
                ],
                accepts_arrivals: true,
            },
        ]
    }

    pub fn arrival_declarations(&self) -> impl Iterator<Item = SourceBindRelation> {
        self.declarations()
            .into_iter()
            .filter(|declaration| declaration.accepts_arrivals)
    }
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct SourceBindRelation {
    pub name: String,
    pub columns: &'static [&'static str],
    pub column_types: &'static [RowColumnType],
    pub accepts_arrivals: bool,
}

#[derive(Clone, Debug)]
pub struct SourceBindFrame {
    pub clock: u64,
    pub envelope: sprefa_rust_runtime_host::SourceHostEnvelope,
    pub arrivals: Vec<Arrival>,
}

#[derive(Clone, Debug)]
pub struct SourceBindTickFrame {
    pub source: SourceBindFrame,
    pub deltas: TickDeltas,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub enum SourceBindError {
    SourceRead { message: String },
    Receipt { message: String },
}

impl std::fmt::Display for SourceBindError {
    fn fmt(&self, formatter: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::SourceRead { message } => {
                write!(formatter, "source extraction read failed: {message}")
            }
            Self::Receipt { message } => write!(formatter, "source receipt failed: {message}"),
        }
    }
}

impl std::error::Error for SourceBindError {}

pub(crate) fn source_row(relation: String, sign: ArrivalSign, row: Vec<Value>) -> Arrival {
    Arrival {
        rel: relation,
        sign,
        row,
    }
}
