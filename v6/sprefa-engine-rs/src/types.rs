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
    List,
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
    // A `list` column's boundary value: the ELEMENTS, parsed once at the read
    // seam (sql.rs). Elements are json values because a rel-typed element is
    // the target's rendered object. Last, so untagged deserialization tries
    // every scalar first.
    List(Vec<serde_json::Value>),
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

// What a binder can take, mirroring IRowScalar. A list column's stored value
// is its interned entity id, an int, so the array arm has no spelling here.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(untagged)]
pub enum ScalarValue {
    Integer(i64),
    Real(f64),
    Bool(bool),
    Text(String),
}

// The binder a list value was asked to cross.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ScalarSeam {
    SqlParameter,
    HostTemplateArgument,
    ArrivalPayload,
    TextIntern,
}

impl ScalarSeam {
    pub fn name(self) -> &'static str {
        match self {
            ScalarSeam::SqlParameter => "a SQL parameter",
            ScalarSeam::HostTemplateArgument => "a host template argument",
            ScalarSeam::ArrivalPayload => "an arrival payload",
            ScalarSeam::TextIntern => "the text intern plane",
        }
    }
}

// The two ways a `list` column's value can be wrong at a runtime boundary,
// plus the recursive head that has no finite least model to reach.
#[derive(Debug, Clone, PartialEq)]
pub enum BoundaryError {
    ListAtScalarSeam(ScalarSeam),
    ListColumnNotAnArray { text: String, detail: String },
    DivergingMeasureRecursion { rel: String, round_cap: u64 },
}

impl std::fmt::Display for BoundaryError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            BoundaryError::ListAtScalarSeam(seam) => {
                write!(f, "a list value reached {}", seam.name())
            }
            BoundaryError::ListColumnNotAnArray { text, detail } => write!(
                f,
                "list column crossed SQLite with non-array text {text}: {detail}"
            ),
            // Same bytes the ts door raises (1_incremental.ts:bounded_wave).
            BoundaryError::DivergingMeasureRecursion { rel, round_cap } => {
                write!(f, "diverging_measure_recursion({rel}, {round_cap})")
            }
        }
    }
}

impl std::error::Error for BoundaryError {}

pub type BoundaryResult<T> = std::result::Result<T, BoundaryError>;

impl ScalarValue {
    pub fn at_seam(value: &Value, seam: ScalarSeam) -> BoundaryResult<ScalarValue> {
        match value {
            Value::Integer(v) => Ok(ScalarValue::Integer(*v)),
            Value::Real(v) => Ok(ScalarValue::Real(*v)),
            Value::Bool(b) => Ok(ScalarValue::Bool(*b)),
            Value::Text(text) => Ok(ScalarValue::Text(text.clone())),
            Value::List(_) => Err(BoundaryError::ListAtScalarSeam(seam)),
        }
    }

    pub fn row_at_seam(row: &[Value], seam: ScalarSeam) -> BoundaryResult<Vec<ScalarValue>> {
        row.iter()
            .map(|value| ScalarValue::at_seam(value, seam))
            .collect()
    }
}

impl From<ScalarValue> for Value {
    fn from(scalar: ScalarValue) -> Value {
        match scalar {
            ScalarValue::Integer(v) => Value::Integer(v),
            ScalarValue::Real(v) => Value::Real(v),
            ScalarValue::Bool(b) => Value::Bool(b),
            ScalarValue::Text(text) => Value::Text(text),
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
    pub params: Vec<ScalarValue>,
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
    pub args: Vec<ScalarValue>,
}

#[derive(Debug, Clone)]
pub struct QueryResult {
    pub rows: Vec<Vec<Value>>,
    pub columns: Vec<String>,
    pub rows_affected: i64,
}

// `rel_columns` carries one flag per relation column, true where the stored
// column holds a dictionary id and the arriving value is its content.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct TextInternPlan {
    pub intern_sql: String,
    pub lookup_sql: String,
    pub rel_columns: std::collections::HashMap<String, Vec<bool>>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct StructTypePlan {
    pub name: String,
    pub columns: Vec<String>,
    pub refs: Vec<Option<String>>,
    pub key_indices: Vec<usize>,
    pub conflict_sql: String,
    pub intern_sql: String,
    pub lookup_sql: String,
}

// One IIncrementalRelationPlan: the per-relation table names and statement
// text the tick engine stages events through.
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
    // lower.pl:fixpoint_round_cap/1. Hops, not rows.
    pub round_cap: u64,
}

// The stratum SCC a level head sits on, strat.pl:cyclic_head_groups/2. Same
// `group` = one mutual cycle, closed by re-running the group's whole pass.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct RecursionGroupPlan {
    pub group: u64,
    // fixpoint_round_cap/1 again, counted in GROUP PASSES rather than hops.
    pub round_cap: u64,
    // The group's head rels, `[path,reach]`; what a tripped cap names.
    pub heads: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct IncrementalLevelStatement {
    pub head_rel: String,
    pub head_delta_table_name: String,
    pub head_columns: Vec<String>,
    pub head_column_types: Vec<RowColumnType>,
    pub insert_sql: Option<String>,
    #[serde(default)]
    pub intern_sql: Option<Vec<String>>,
    pub select_sql: String,
    pub recompute_delete_sql: String,
    pub recompute_insert_sqls: Vec<String>,
    pub recompute_sql: String,
    pub support_sql: Option<Vec<String>>,
    pub support_intern_sql: Option<Vec<String>>,
    pub expand_sql: Option<ExpandPlan>,
    pub dred_sql: Option<DredPlan>,
    // None on an acyclic head, and on every module emitted before outer rounds.
    #[serde(default)]
    pub recursion_group: Option<RecursionGroupPlan>,
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
    #[serde(default)]
    pub intern_sql: Option<Vec<String>>,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum OrderedTriggerKind {
    Arrival,
    Departure,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct OrderedEdgeArm {
    pub trigger_rel: String,
    pub trigger_kind: OrderedTriggerKind,
    pub head_rel: String,
    pub head_kind: RelationKind,
    pub head_columns: Vec<String>,
    pub key_indices: Vec<usize>,
    pub project_sql: String,
    pub write_sql: String,
    pub evolves_pre: bool,
    pub intern_sql: Option<Vec<String>>,
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

// One host column as the emitter spells it; `type` is the declared column
// type name (text/int/float/bool/json or a declared struct name).
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct HostColumnPlan {
    pub name: String,
    #[serde(rename = "type")]
    pub column_type: String,
}

// Mirrors emit_ts.pl's IHostPlanData row; the two runtimes read one
// executor contract.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct HostPlanData {
    pub name: String,
    pub inputs: Vec<HostColumnPlan>,
    pub outputs: Vec<HostColumnPlan>,
    pub template: String,
    pub demand_rel: String,
    pub response_rel: String,
    pub execution: String,
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
    #[serde(default)]
    pub struct_types: Vec<StructTypePlan>,
    #[serde(default)]
    pub struct_ref_columns: std::collections::HashMap<String, Vec<Option<String>>>,
    #[serde(default)]
    pub ordered_program: bool,
    #[serde(default)]
    pub ordered_arms: Vec<OrderedEdgeArm>,
    #[serde(default)]
    pub ordered_pre_refs: Vec<String>,
    #[serde(default)]
    pub ordered_recursive_levels: bool,
    pub relations: Vec<IncrementalRelationPlan>,
    pub edges: Vec<IncrementalEdgeStatement>,
    pub levels: Vec<IncrementalLevelStatement>,
    pub retentions: Vec<IncrementalRetentionStatement>,
    #[serde(default)]
    pub uses_tick: bool,
    pub reconcile_every_tick: bool,
    pub incremental_safe: bool,
    #[serde(default)]
    pub host_plans: Vec<HostPlanData>,
}
