#[derive(Debug, Clone, PartialEq, serde::Serialize, serde::Deserialize)]
pub struct Line {
    pub name: String,
    pub body: String,
}

#[derive(Debug, Clone, PartialEq, serde::Serialize, serde::Deserialize)]
pub struct LineParts {
    pub name: String,
    pub parts: i64,
}
