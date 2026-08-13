#[derive(Debug, Clone, PartialEq, serde::Serialize, serde::Deserialize)]
pub struct DfArg {
    pub caller_path: String,
    pub call_start: i64,
    pub call_end: i64,
    pub pos: i64,
    pub arg: String,
    pub arg_end: i64,
}

#[derive(Debug, Clone, PartialEq, serde::Serialize, serde::Deserialize)]
pub struct DfParam {
    pub callee_path: String,
    pub param: String,
    pub pos: i64,
    pub param_end: i64,
}

#[derive(Debug, Clone, PartialEq, serde::Serialize, serde::Deserialize)]
pub struct FlowEdge {
    pub from: String,
    pub to: String,
}

#[derive(Debug, Clone, PartialEq, serde::Serialize, serde::Deserialize)]
pub struct ResolvedCallEdge {
    pub caller_path: String,
    pub call_start: i64,
    pub call_end: i64,
    pub callee_path: String,
    pub callee_name: String,
}
