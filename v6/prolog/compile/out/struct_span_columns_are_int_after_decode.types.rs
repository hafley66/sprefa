#[derive(Debug, Clone, PartialEq, serde::Serialize, serde::Deserialize)]
pub struct DefStart {
    pub path: String,
    pub offset: i64,
}

#[derive(Debug, Clone, PartialEq, serde::Serialize, serde::Deserialize)]
pub struct NodeFact {
    pub path: String,
    pub name: String,
    pub at: Span,
}

#[derive(Debug, Clone, PartialEq, serde::Serialize, serde::Deserialize)]
pub struct Span {
    pub end: i64,
    pub start: i64,
}
