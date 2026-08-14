#[derive(Debug, Clone, PartialEq, serde::Serialize, serde::Deserialize)]
pub struct Mark {
    pub at: Span,
}

#[derive(Debug, Clone, PartialEq, serde::Serialize, serde::Deserialize)]
pub struct Span {
    pub start: i64,
    pub end: i64,
}
