#[derive(Debug, Clone, PartialEq, serde::Serialize, serde::Deserialize)]
pub struct CallEdge {
    pub file: String,
    pub caller: String,
    pub callee: String,
    pub start: i64,
    pub end: i64,
}

#[derive(Debug, Clone, PartialEq, serde::Serialize, serde::Deserialize)]
pub struct CallSite {
    pub file: String,
    pub caller: String,
    pub callee: String,
}

#[derive(Debug, Clone, PartialEq, serde::Serialize, serde::Deserialize)]
pub struct File {
    pub file: String,
    pub file_digest: String,
}

#[derive(Debug, Clone, PartialEq, serde::Serialize, serde::Deserialize)]
pub struct QueryValue {
    pub query_digest: String,
}
