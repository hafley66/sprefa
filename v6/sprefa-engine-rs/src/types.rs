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
    #[serde(rename = "relation_id")]
    RelationId,
    List,
    Bytes,
}

// A row value as it crosses a runtime boundary. Mirrors the values IRowValue
// holds at boundary time in the TS runtime: integers, floats, booleans, and
// text (json/document text rides as Text).

#[derive(Debug, Clone, PartialEq)]
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
    Bytes(Vec<u8>),
}

pub(crate) fn bytes_to_base64(bytes: &[u8]) -> String {
    const ALPHABET: &[u8; 64] = b"ABCDEFGHIJKLMNOPQRSTUVWXYZabcdefghijklmnopqrstuvwxyz0123456789+/";
    let mut out = String::new();
    for chunk in bytes.chunks(3) {
        let a = chunk[0];
        let b = *chunk.get(1).unwrap_or(&0);
        let c = *chunk.get(2).unwrap_or(&0);
        out.push(ALPHABET[(a >> 2) as usize] as char);
        out.push(ALPHABET[(((a & 3) << 4) | (b >> 4)) as usize] as char);
        out.push(if chunk.len() > 1 {
            ALPHABET[((b & 15) << 2 | c >> 6) as usize] as char
        } else {
            '='
        });
        out.push(if chunk.len() > 2 {
            ALPHABET[(c & 63) as usize] as char
        } else {
            '='
        });
    }
    out
}

pub(crate) fn base64_to_bytes(text: &str) -> Result<Vec<u8>, String> {
    if !text.len().is_multiple_of(4) {
        return Err("invalid_bytes_base64".into());
    }
    let value = |byte: u8| match byte {
        b'A'..=b'Z' => Some(byte - b'A'),
        b'a'..=b'z' => Some(byte - b'a' + 26),
        b'0'..=b'9' => Some(byte - b'0' + 52),
        b'+' => Some(62),
        b'/' => Some(63),
        _ => None,
    };
    let mut out = Vec::with_capacity(text.len() / 4 * 3);
    for chunk in text.as_bytes().chunks(4) {
        let a = value(chunk[0]).ok_or("invalid_bytes_base64")?;
        let b = value(chunk[1]).ok_or("invalid_bytes_base64")?;
        let c = if chunk[2] == b'=' {
            0
        } else {
            value(chunk[2]).ok_or("invalid_bytes_base64")?
        };
        let d = if chunk[3] == b'=' {
            0
        } else {
            value(chunk[3]).ok_or("invalid_bytes_base64")?
        };
        if chunk[2] == b'=' && chunk[3] != b'=' {
            return Err("invalid_bytes_base64".into());
        }
        out.push((a << 2) | (b >> 4));
        if chunk[2] != b'=' {
            out.push((b << 4) | (c >> 2));
        }
        if chunk[3] != b'=' {
            out.push((c << 6) | d);
        }
    }
    if bytes_to_base64(&out) != text {
        return Err("invalid_bytes_base64".into());
    }
    Ok(out)
}

fn serialize_value_json(value: &Value) -> serde_json::Value {
    match value {
        Value::Integer(v) => serde_json::json!(v),
        Value::Real(v) => serde_json::json!(v),
        Value::Bool(v) => serde_json::json!(v),
        Value::Text(v) => serde_json::json!(v),
        Value::List(v) => serde_json::Value::Array(v.clone()),
        Value::Bytes(v) => serde_json::json!({"$bytes": bytes_to_base64(v)}),
    }
}

impl Serialize for Value {
    fn serialize<S: serde::Serializer>(&self, serializer: S) -> Result<S::Ok, S::Error> {
        serialize_value_json(self).serialize(serializer)
    }
}

impl<'de> Deserialize<'de> for Value {
    fn deserialize<D: serde::Deserializer<'de>>(deserializer: D) -> Result<Self, D::Error> {
        let value = serde_json::Value::deserialize(deserializer)?;
        match value {
            serde_json::Value::Number(n) => n
                .as_i64()
                .map(Value::Integer)
                .or_else(|| n.as_f64().map(Value::Real))
                .ok_or_else(|| serde::de::Error::custom("invalid number")),
            serde_json::Value::Bool(v) => Ok(Value::Bool(v)),
            serde_json::Value::String(v) => Ok(Value::Text(v)),
            serde_json::Value::Array(v) => Ok(Value::List(v)),
            serde_json::Value::Object(mut v) if v.len() == 1 && v.contains_key("$bytes") => {
                let encoded = v
                    .remove("$bytes")
                    .and_then(|v| v.as_str().map(str::to_owned))
                    .ok_or_else(|| serde::de::Error::custom("invalid bytes tag"))?;
                base64_to_bytes(&encoded)
                    .map(Value::Bytes)
                    .map_err(serde::de::Error::custom)
            }
            other => Err(serde::de::Error::custom(format!(
                "unsupported value {other}"
            ))),
        }
    }
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
#[derive(Debug, Clone, PartialEq)]
pub enum ScalarValue {
    Integer(i64),
    Real(f64),
    Bool(bool),
    Text(String),
    Bytes(Vec<u8>),
}

impl Serialize for ScalarValue {
    fn serialize<S: serde::Serializer>(&self, serializer: S) -> Result<S::Ok, S::Error> {
        let value = match self {
            ScalarValue::Integer(v) => serde_json::json!(v),
            ScalarValue::Real(v) => serde_json::json!(v),
            ScalarValue::Bool(v) => serde_json::json!(v),
            ScalarValue::Text(v) => serde_json::json!(v),
            ScalarValue::Bytes(v) => serde_json::json!({"$bytes": bytes_to_base64(v)}),
        };
        value.serialize(serializer)
    }
}

impl<'de> Deserialize<'de> for ScalarValue {
    fn deserialize<D: serde::Deserializer<'de>>(deserializer: D) -> Result<Self, D::Error> {
        let value = serde_json::Value::deserialize(deserializer)?;
        match value {
            serde_json::Value::Number(n) => n
                .as_i64()
                .map(ScalarValue::Integer)
                .or_else(|| n.as_f64().map(ScalarValue::Real))
                .ok_or_else(|| serde::de::Error::custom("invalid number")),
            serde_json::Value::Bool(v) => Ok(ScalarValue::Bool(v)),
            serde_json::Value::String(v) => Ok(ScalarValue::Text(v)),
            serde_json::Value::Object(mut v) if v.len() == 1 && v.contains_key("$bytes") => {
                let encoded = v
                    .remove("$bytes")
                    .and_then(|v| v.as_str().map(str::to_owned))
                    .ok_or_else(|| serde::de::Error::custom("invalid bytes tag"))?;
                base64_to_bytes(&encoded)
                    .map(ScalarValue::Bytes)
                    .map_err(serde::de::Error::custom)
            }
            other => Err(serde::de::Error::custom(format!(
                "unsupported scalar {other}"
            ))),
        }
    }
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
    BytesAtScalarSeam(ScalarSeam),
    ListColumnNotAnArray { text: String, detail: String },
    DivergingMeasureRecursion { rel: String, round_cap: u64 },
}

impl std::fmt::Display for BoundaryError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            BoundaryError::ListAtScalarSeam(seam) => {
                write!(f, "a list value reached {}", seam.name())
            }
            BoundaryError::BytesAtScalarSeam(seam) => {
                write!(f, "bytes reached {}", seam.name())
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
            Value::Bytes(bytes) => match seam {
                ScalarSeam::SqlParameter => Ok(ScalarValue::Bytes(bytes.clone())),
                _ => Err(BoundaryError::BytesAtScalarSeam(seam)),
            },
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
            ScalarValue::Bytes(bytes) => Value::Bytes(bytes),
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

// The public tagged-value schema for an enum whose SQLite representation is an
// INTEGER endpoint.  Variant rows retain their ordinary physical `id` plus
// payload columns; this plan is the sole translation authority at the runtime
// boundary.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct EnumVariantPlan {
    pub tag: String,
    pub rel: String,
    pub fields: Vec<String>,
    #[serde(default)]
    pub field_types: Vec<RowColumnType>,
    #[serde(default)]
    pub field_enums: Vec<Option<String>>,
    #[serde(default)]
    pub select_sql: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct EnumTypePlan {
    pub name: String,
    pub variants: Vec<EnumVariantPlan>,
    #[serde(default)]
    pub identity: Option<EnumIdentityPlan>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct EnumIdentityPlan {
    pub intern_sql: String,
    pub lookup_sql: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct EnumRefColumn {
    pub name: String,
    pub endpoint_index: Option<usize>,
}

pub type EnumRefColumns = std::collections::HashMap<String, Vec<Option<EnumRefColumn>>>;

// Present only under frontier(shared): this rel's row in the two shared
// frontier tables; frontier/next table names then name TEMP views.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SharedFrontierPlan {
    pub relation_id: i64,
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
    #[serde(default)]
    pub shared_frontier: Option<SharedFrontierPlan>,
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
    pub head_table_name: String,
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
    // frontier(shared) only: the recount verb's shared arm, publishing this
    // head's per-rule support to the shared ledger after the head insert.
    #[serde(default)]
    pub support_count_sql: Option<WriteSupportCountPlan>,
    pub expand_sql: Option<ExpandPlan>,
    pub dred_sql: Option<DredPlan>,
    // None on an acyclic head, and on every module emitted before outer rounds.
    #[serde(default)]
    pub recursion_group: Option<RecursionGroupPlan>,
    pub aggregate_sql: Option<AggregateLevelPlan>,
}

// lower.pl:support_count_plan/8. clear_sql empties this rel's rows in the
// shared ledger, write_sqls refill them, one statement per rule.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct WriteSupportCountPlan {
    pub clear_sql: String,
    pub write_sqls: Vec<String>,
}

/// Sequenced = the body reads the store this tick is still writing (`pre/1`, or
/// a negation over a rel another arm heads). Per ARM, never per module.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum ArmSchedule {
    #[default]
    SetAtOnce,
    Sequenced,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Default, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum TriggerKind {
    #[default]
    Arrival,
    Departure,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct IncrementalEdgeStatement {
    pub head_rel: String,
    pub head_columns: Vec<String>,
    pub head_table_name: String,
    pub head_kind: RelationKind,
    pub key_indices: Vec<usize>,
    /// The set-at-once projection: the whole trigger frontier in one statement.
    pub project_sql: String,
    #[serde(default)]
    pub intern_sql: Option<Vec<String>>,
    #[serde(default)]
    pub schedule: ArmSchedule,
    #[serde(default)]
    pub trigger_rel: String,
    #[serde(default)]
    pub trigger_kind: TriggerKind,
    /// One occurrence's projection, the trigger row bound to `?1..?n`. Present
    /// exactly when `schedule` is `Sequenced`.
    #[serde(default)]
    pub occurrence_project_sql: Option<String>,
    #[serde(default)]
    pub occurrence_intern_sql: Option<Vec<String>>,
    /// Some body reads this head through `pre/1`, so each write also lands in
    /// `__pre_<head>` for the occurrences still to come.
    #[serde(default)]
    pub evolves_pre: bool,
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

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct HostTypeField {
    pub name: String,
    #[serde(rename = "type")]
    pub field_type: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct HostTypeDescriptor {
    #[serde(rename = "ref")]
    pub type_ref: String,
    pub fields: Vec<HostTypeField>,
}

// Mirrors emit_ts.pl's IHostPlanData row; the two runtimes read one
// executor contract. Structured plans carry optional request/response
// descriptors; omission is the legacy scalar shell-host shape.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct HostPlanData {
    pub name: String,
    pub inputs: Vec<HostColumnPlan>,
    pub outputs: Vec<HostColumnPlan>,
    pub template: String,
    pub demand_rel: String,
    pub response_rel: String,
    pub execution: String,
    #[serde(default)]
    pub request_type: Option<HostTypeDescriptor>,
    #[serde(default)]
    pub response_type: Option<HostTypeDescriptor>,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct HostAdapterRow {
    pub adapter: String,
    pub demand_rel: String,
    pub response_rel: String,
}

/// One answered row at the host seam, keyed by declared column name. A linked
/// executor builds these; a host answer never crosses this seam as text.
pub type HostRow = serde_json::Map<String, serde_json::Value>;

pub fn load_host_adapter_rows(
    path: impl AsRef<std::path::Path>,
) -> std::io::Result<Vec<HostAdapterRow>> {
    let text = std::fs::read_to_string(path)?;
    serde_json::from_str(&text).map_err(std::io::Error::other)
}

pub fn load_program_host_adapter_rows(program: &str) -> std::io::Result<Vec<HostAdapterRow>> {
    let directory = std::env::var_os("DL_ADAPTERS_DIR")
        .map(std::path::PathBuf::from)
        .unwrap_or_else(|| {
            std::path::PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("../dl/fixtures")
        });
    let path = directory.join(format!("{program}.adapters.json"));
    match load_host_adapter_rows(path) {
        Ok(rows) => Ok(rows),
        Err(error) if error.kind() == std::io::ErrorKind::NotFound => Ok(Vec::new()),
        Err(error) => Err(error),
    }
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
    pub enum_types: Vec<EnumTypePlan>,
    #[serde(default)]
    pub enum_ref_columns: EnumRefColumns,
    /// The rels a `pre/1` body reads, each mirrored into `__pre_<table>` at the
    /// tick's edge boundary.
    #[serde(default)]
    pub pre_snapshot_rels: Vec<String>,
    pub relations: Vec<IncrementalRelationPlan>,
    pub edges: Vec<IncrementalEdgeStatement>,
    pub levels: Vec<IncrementalLevelStatement>,
    pub retentions: Vec<IncrementalRetentionStatement>,
    #[serde(default)]
    pub uses_tick: bool,
    pub reconcile_every_tick: bool,
    // Nothing reads this: emit_rust.pl writes the constant true and
    // program.rs only copies it across. Required-on-read broke every
    // snapshot emitted before it existed and checked nothing.
    #[serde(default)]
    pub incremental_safe: bool,
    // Absent on an IR emitted before the field existed; 0 then fails
    // program::GenProgram::try_from_json's named check.
    #[serde(default)]
    pub ir_version: u32,
    #[serde(default)]
    pub host_plans: Vec<HostPlanData>,
    /// The `?` query names in declared order. Empty on every IR emitted before
    /// the emitter wrote them.
    #[serde(default)]
    pub queries: Vec<String>,
}
