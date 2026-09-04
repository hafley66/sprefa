// sprefa:auto-begin dl7-runtime-types
// generated from v7/schema/0_runtime_types.dl7
// source of truth: DL7 product and sum edges

#[derive(Clone, Debug, PartialEq, serde::Serialize, serde::Deserialize)]
pub enum BatchMode {
    Snapshot,
    Delta,
}

#[derive(Clone, Debug, PartialEq, serde::Serialize, serde::Deserialize)]
pub struct SourceCoordinate {
    pub repository: String,
    pub worktree: String,
    pub path: String,
    pub content: String,
}

#[derive(Clone, Debug, PartialEq, serde::Serialize, serde::Deserialize)]
pub struct TsiObservation {
    pub relation: String,
    pub args: serde_json::Value,
}

#[derive(Clone, Debug, PartialEq, serde::Serialize, serde::Deserialize)]
pub struct WatchChange {
    pub generation: u64,
    pub sign: i8,
    pub source: SourceCoordinate,
    pub observation: TsiObservation,
}

#[derive(Clone, Debug, PartialEq, serde::Serialize, serde::Deserialize)]
pub struct DbspRelationShape {
    pub name: String,
    pub arity: usize,
    pub input: bool,
    pub output: bool,
}

#[derive(Clone, Debug, PartialEq, serde::Serialize, serde::Deserialize)]
pub struct DbspOperatorShape {
    pub id: String,
    pub kind: String,
    pub head: String,
    pub body: serde_json::Value,
}

#[derive(Clone, Debug, PartialEq, serde::Serialize, serde::Deserialize)]
pub struct DbspPlanShape {
    pub ir_version: u32,
    pub runtime: String,
    pub relations: serde_json::Value,
    pub operators: serde_json::Value,
}
// sprefa:auto-end dl7-runtime-types
