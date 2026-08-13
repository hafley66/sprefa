#[derive(Debug, Clone, PartialEq, serde::Serialize, serde::Deserialize)]
pub struct DfParam {
    pub path: String,
    pub node: String,
    pub pos: i64,
    pub end: i64,
}

#[derive(Debug, Clone, PartialEq, serde::Serialize, serde::Deserialize)]
pub struct FlowNodeType {
    pub node: String,
    pub path: String,
    pub name: String,
    pub ty: String,
}

#[derive(Debug, Clone, PartialEq, serde::Serialize, serde::Deserialize)]
pub struct FlowParamType {
    pub path: String,
    pub name: String,
    pub pos: i64,
    pub ty: String,
}

#[derive(Debug, Clone, PartialEq, serde::Serialize, serde::Deserialize)]
pub struct Sig {
    pub path: String,
    pub owner_start: i64,
    pub owner_end: i64,
    pub slot: String,
    pub pos: i64,
    pub ty: String,
}

#[derive(Debug, Clone, PartialEq, serde::Serialize, serde::Deserialize)]
pub struct SinkCallee {
    pub path: String,
    pub name: String,
}

#[derive(Debug, Clone, PartialEq, serde::Serialize, serde::Deserialize)]
pub struct TypeOwner {
    pub path: String,
    pub name: String,
    pub start: i64,
    pub end: i64,
}
