// Declared column types a row column can carry. Mirrors IRowColumnType in the
// TS runtime; drives ticklog encoding and boundary normalization.

use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum RowColumnType {
    Text,
    Int,
    Float,
    Bool,
    Json,
    Ref,
}

impl RowColumnType {
    pub fn parse(name: &str) -> Self {
        match name {
            "text" => RowColumnType::Text,
            "int" => RowColumnType::Int,
            "float" => RowColumnType::Float,
            "bool" => RowColumnType::Bool,
            "json" => RowColumnType::Json,
            "ref" => RowColumnType::Ref,
            _ => RowColumnType::Text,
        }
    }
}

// A row value as it crosses a runtime boundary. Mirrors the values IRowValue
// holds at boundary time in the TS runtime: integers, floats, booleans, and
// text (json/document text rides as Text).

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(untagged)]
pub enum Value {
    Integer(i64),
    Real(f64),
    Bool(bool),
    Text(String),
}

impl Value {
    pub fn as_i64(&self) -> Option<i64> {
        match self {
            Value::Integer(v) => Some(*v),
            Value::Real(v) if *v == v.trunc() => Some(*v as i64),
            _ => None,
        }
    }
}

pub type Row = Vec<Value>;

#[derive(Debug, Clone)]
pub struct Arrival {
    pub rel: String,
    pub sign: ArrivalSign,
    pub row: Row,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ArrivalSign {
    Add,
    Del,
}

#[derive(Debug, Clone, Default, Serialize, Deserialize)]
pub struct RelDelta {
    pub rel: String,
    pub add: Vec<Row>,
    pub del: Vec<Row>,
}

#[derive(Debug, Clone, Default)]
pub struct TickDeltas {
    pub rels: Vec<RelDelta>,
    pub carry_pending: bool,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct BootStatement {
    pub rel: String,
    pub sql: String,
    pub params: Vec<Value>,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum InternMode {
    Dict,
    Direct,
}

impl InternMode {
    pub fn parse(name: &str) -> Self {
        match name {
            "dict" => InternMode::Dict,
            _ => InternMode::Direct,
        }
    }
}

// A prepared SQL statement plus its bound args, the unit the seam executes.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SqlStatement {
    pub sql: String,
    pub args: Vec<Value>,
}

#[derive(Debug, Clone)]
pub struct QueryResult {
    pub rows: Vec<Vec<Value>>,
    pub columns: Vec<String>,
    pub rows_affected: i64,
}

// One IIncrementalRelationPlan: the per-relation table names and statement
// text the tick engine stages events through.
// `rel_columns` carries one flag per relation column, true where the stored
// column holds a dictionary id and the arriving value is its content.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct TextInternPlan {
    pub intern_sql: String,
    pub lookup_sql: String,
    pub rel_columns: std::collections::HashMap<String, Vec<bool>>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct IncrementalRelationPlan {
    pub rel: String,
    pub kind: RelationKind,
    pub table_name: String,
    pub delta_table_name: String,
    pub frontier_table_name: String,
    pub next_frontier_table_name: String,
    pub departure_frontier_table_name: Option<String>,
    pub columns: Vec<String>,
    pub column_types: Vec<RowColumnType>,
    pub key_indices: Vec<usize>,
    pub arrival_add_sql: Option<String>,
    pub arrival_del_sql: Option<String>,
    pub boundary_sql: String,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum RelationKind {
    Set,
    Log,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct DredPlan {
    pub clear_ping_sql: String,
    pub clear_pong_sql: String,
    pub clear_cone_sql: String,
    pub assert_seed_sqls: Vec<String>,
    pub assert_hop_ab_sql: String,
    pub assert_hop_ba_sql: String,
    pub commit_a_sql: String,
    pub commit_b_sql: String,
    pub arrival_a_sql: String,
    pub arrival_b_sql: String,
    pub dred_seed_sqls: Vec<String>,
    pub dred_hop_ab_sql: String,
    pub dred_hop_ba_sql: String,
    pub cone_absorb_a_sql: String,
    pub cone_absorb_b_sql: String,
    pub cone_trim_sql: String,
    pub head_delete_sql: String,
    pub rederive_seed_sqls: Vec<String>,
    pub revive_hop_ab_sql: String,
    pub revive_hop_ba_sql: String,
    pub cone_drop_a_sql: String,
    pub cone_drop_b_sql: String,
    pub stage_retract_sql: String,
    pub head_count_sql: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct AggregateLevelPlan {
    pub scope_clear_sql: String,
    pub scope_seed_sql: Vec<String>,
    pub intern_sql: Option<Vec<String>>,
    pub delete_scoped_sql: String,
    pub insert_scoped_sql: Vec<String>,
    pub delta_maintained: bool,
}

// SQL statements for one expand wavefront plan.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ExpandPlan {
    pub clear_a_sql: String,
    pub clear_b_sql: String,
    pub seed_sqls: Vec<String>,
    pub hop_ab_sql: String,
    pub hop_ba_sql: String,
    pub absorb_a_sql: String,
    pub absorb_b_sql: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct IncrementalLevelStatement {
    pub head_rel: String,
    pub head_delta_table_name: String,
    pub head_columns: Vec<String>,
    pub head_column_types: Vec<RowColumnType>,
    pub insert_sql: Option<String>,
    pub select_sql: String,
    pub recompute_sql: String,
    pub support_sql: Option<Vec<String>>,
    pub support_intern_sql: Option<Vec<String>>,
    pub expand_sql: Option<ExpandPlan>,
    pub dred_sql: Option<DredPlan>,
    pub aggregate_sql: Option<AggregateLevelPlan>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct IncrementalEdgeStatement {
    pub head_rel: String,
    pub head_columns: Vec<String>,
    pub head_table_name: String,
    pub head_kind: RelationKind,
    pub key_indices: Vec<usize>,
    pub project_sql: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct IncrementalRetentionStatement {
    pub rel: String,
    pub delete_sql: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ArrivalTemplate {
    pub kind: RelationKind,
    pub add_sql: String,
    pub del_sql: Option<String>,
}

// The serde mirror of the emitted program: one JSON object per fixture.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ProgramJson {
    pub name: String,
    pub intern_mode: InternMode,
    pub ddl: Vec<String>,
    pub rel_columns: std::collections::HashMap<String, Vec<String>>,
    pub rel_column_types: std::collections::HashMap<String, Vec<RowColumnType>>,
    pub arrival_targets: Vec<String>,
    pub boot: Vec<BootStatement>,
    pub final_select: std::collections::HashMap<String, String>,
    pub arrival_templates: std::collections::HashMap<String, ArrivalTemplate>,
    #[serde(default)]
    pub text_intern_plan: Option<TextInternPlan>,
    pub relations: Vec<IncrementalRelationPlan>,
    pub edges: Vec<IncrementalEdgeStatement>,
    pub levels: Vec<IncrementalLevelStatement>,
    pub retentions: Vec<IncrementalRetentionStatement>,
    pub reconcile_every_tick: bool,
    pub incremental_safe: bool,
}
