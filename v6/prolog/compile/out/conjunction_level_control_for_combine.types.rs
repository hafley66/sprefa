#[derive(Debug, Clone, PartialEq, serde::Serialize, serde::Deserialize)]
pub struct Pair {
    pub left: i64,
    pub right: i64,
}

#[derive(Debug, Clone, PartialEq, serde::Serialize, serde::Deserialize)]
pub struct SourceA {
    pub left: i64,
}

#[derive(Debug, Clone, PartialEq, serde::Serialize, serde::Deserialize)]
pub struct SourceB {
    pub right: i64,
}
