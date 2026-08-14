#[derive(Debug, Clone, PartialEq, serde::Serialize, serde::Deserialize)]
pub struct ArtifactPlan {
    pub name: String,
    pub target: EmitterTarget,
    pub program: ProgramPlan,
    pub format: String,
    pub path: String,
    pub linkage: String,
    pub load_phase: String,
}

#[derive(Debug, Clone, PartialEq, serde::Serialize, serde::Deserialize)]
pub struct BindPlan {
    pub name: String,
    pub source_kind: String,
    pub target_relation: RelationPlan,
    pub execution: String,
}

#[derive(Debug, Clone, PartialEq, serde::Serialize, serde::Deserialize)]
pub struct DeploymentArtifact {
    pub deployment_name: String,
    pub ordinal: i64,
    pub artifact: ArtifactPlan,
}

#[derive(Debug, Clone, PartialEq, serde::Serialize, serde::Deserialize)]
pub struct DeploymentPlan {
    pub name: String,
    pub runtime: RuntimeBed,
    pub assembly_kind: String,
    pub binary_count: i64,
    pub dynamic_loader: Option<String>,
}

#[derive(Debug, Clone, PartialEq, serde::Serialize, serde::Deserialize)]
pub struct EmitterTarget {
    pub name: String,
    pub language: String,
    pub artifact_kind: String,
    pub execution_kind: String,
    pub assembly_kind: String,
}

#[derive(Debug, Clone, PartialEq, serde::Serialize, serde::Deserialize)]
pub struct EventPlan {
    pub name: String,
    pub source: ModulePlan,
    pub target: ModulePlan,
    pub delivery: String,
    pub cardinality: String,
    pub lifetime: String,
}

#[derive(Debug, Clone, PartialEq, serde::Serialize, serde::Deserialize)]
pub struct HostPlan {
    pub name: String,
    pub demand_relation: RelationPlan,
    pub response_relation: RelationPlan,
    pub execution: String,
}

#[derive(Debug, Clone, PartialEq, serde::Serialize, serde::Deserialize)]
pub struct LoweringIntent {
    pub name: String,
    pub target: EmitterTarget,
    pub kind: String,
    pub input_kind: String,
    pub output_kind: String,
}

#[derive(Debug, Clone, PartialEq, serde::Serialize, serde::Deserialize)]
pub struct ModulePlan {
    pub name: String,
    pub role: String,
    pub artifact_path: String,
    pub load_phase: String,
    pub entry: bool,
}

#[derive(Debug, Clone, PartialEq, serde::Serialize, serde::Deserialize)]
pub struct ProgramBind {
    pub program_name: String,
    pub ordinal: i64,
    pub bind: BindPlan,
}

#[derive(Debug, Clone, PartialEq, serde::Serialize, serde::Deserialize)]
pub struct ProgramHost {
    pub program_name: String,
    pub ordinal: i64,
    pub host: HostPlan,
}

#[derive(Debug, Clone, PartialEq, serde::Serialize, serde::Deserialize)]
pub struct ProgramPlan {
    pub name: String,
    pub target: EmitterTarget,
    pub entry_module: ModulePlan,
    pub subscription: SubscriptionPlan,
    pub intern_mode: String,
}

#[derive(Debug, Clone, PartialEq, serde::Serialize, serde::Deserialize)]
pub struct ProgramQuery {
    pub program_name: String,
    pub ordinal: i64,
    pub query: QueryPlan,
}

#[derive(Debug, Clone, PartialEq, serde::Serialize, serde::Deserialize)]
pub struct ProgramRelation {
    pub program_name: String,
    pub ordinal: i64,
    pub relation: RelationPlan,
}

#[derive(Debug, Clone, PartialEq, serde::Serialize, serde::Deserialize)]
pub struct ProgramStatement {
    pub program_name: String,
    pub ordinal: i64,
    pub statement: StatementPlan,
}

#[derive(Debug, Clone, PartialEq, serde::Serialize, serde::Deserialize)]
pub struct QueryPlan {
    pub name: String,
    pub relation: RelationPlan,
    pub snapshot: String,
    pub transport_path: String,
}

#[derive(Debug, Clone, PartialEq, serde::Serialize, serde::Deserialize)]
pub struct RelationPlan {
    pub name: String,
    pub arity: i64,
    pub storage_kind: String,
    pub arrival_target: bool,
    pub subscribed: bool,
}

#[derive(Debug, Clone, PartialEq, serde::Serialize, serde::Deserialize)]
pub struct RuntimeBed {
    pub name: String,
    pub target: EmitterTarget,
    pub storage: StoragePlan,
    pub server: ServerPlan,
}

#[derive(Debug, Clone, PartialEq, serde::Serialize, serde::Deserialize)]
pub struct RuntimeEvent {
    pub runtime_name: String,
    pub ordinal: i64,
    pub event: EventPlan,
}

#[derive(Debug, Clone, PartialEq, serde::Serialize, serde::Deserialize)]
pub struct RuntimeModule {
    pub runtime_name: String,
    pub ordinal: i64,
    pub module: ModulePlan,
}

#[derive(Debug, Clone, PartialEq, serde::Serialize, serde::Deserialize)]
pub struct RustDynTarget {
    pub target: EmitterTarget,
    pub rust_runtime: String,
    pub sqlite_backend: String,
    pub generated_module_kind: String,
    pub linkage: String,
    pub dynamic_loader: Option<String>,
}

#[derive(Debug, Clone, PartialEq, serde::Serialize, serde::Deserialize)]
pub struct RustIrTarget {
    pub target: EmitterTarget,
    pub rust_runtime: String,
    pub sqlite_backend: String,
    pub ir_format: String,
    pub request_boundary: String,
}

#[derive(Debug, Clone, PartialEq, serde::Serialize, serde::Deserialize)]
pub struct ServerPlan {
    pub harness: String,
    pub transport: String,
    pub program_cardinality: String,
    pub process_lifetime: String,
}

#[derive(Debug, Clone, PartialEq, serde::Serialize, serde::Deserialize)]
pub struct StatementPlan {
    pub name: String,
    pub phase: String,
    pub relation: RelationPlan,
    pub sql: String,
    pub params_shape: serde_json::Value,
}

#[derive(Debug, Clone, PartialEq, serde::Serialize, serde::Deserialize)]
pub struct StoragePlan {
    pub backend: String,
    pub database_scope: String,
    pub transaction_owner: String,
    pub state_kind: String,
}

#[derive(Debug, Clone, PartialEq, serde::Serialize, serde::Deserialize)]
pub struct SubscriptionPlan {
    pub root_relation: RelationPlan,
    pub activation: String,
    pub release: String,
    pub cone_kind: String,
}

#[derive(Debug, Clone, PartialEq, serde::Serialize, serde::Deserialize)]
pub struct TargetIntent {
    pub target_name: String,
    pub ordinal: i64,
    pub intent: LoweringIntent,
}

#[derive(Debug, Clone, PartialEq, serde::Serialize, serde::Deserialize)]
pub struct Tsv2Target {
    pub target: EmitterTarget,
    pub typescript_runtime: String,
    pub sqlite_backend: String,
    pub module_format: String,
}
