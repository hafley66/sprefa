#[derive(Debug, Clone, PartialEq, serde::Serialize, serde::Deserialize)]
pub struct HostSpan {
    pub path: String,
    pub at: Span,
}

#[derive(Debug, Clone, PartialEq, serde::Serialize, serde::Deserialize)]
pub struct HostStart {
    pub path: String,
    pub start: i64,
}

#[derive(Debug, Clone, PartialEq, serde::Serialize, serde::Deserialize)]
pub struct SourcePath {
    pub path: String,
}

#[derive(Debug, Clone, PartialEq, serde::Serialize, serde::Deserialize)]
pub struct Span {
    pub end: i64,
    pub start: i64,
}
