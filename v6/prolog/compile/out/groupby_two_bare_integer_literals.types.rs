#[derive(Debug, Clone, PartialEq, serde::Serialize, serde::Deserialize)]
pub struct Classified {
    pub name: String,
    pub line: i64,
    pub column: i64,
}

#[derive(Debug, Clone, PartialEq, serde::Serialize, serde::Deserialize)]
pub struct Source {
    pub name: String,
}
