#[derive(Debug, Clone, PartialEq, serde::Serialize, serde::Deserialize)]
pub struct Hit {
    pub path: String,
    pub line: i64,
    pub col3: i64,
}

#[derive(Debug, Clone, PartialEq, serde::Serialize, serde::Deserialize)]
pub struct Hits {
    pub path: String,
    pub col2: i64,
}
