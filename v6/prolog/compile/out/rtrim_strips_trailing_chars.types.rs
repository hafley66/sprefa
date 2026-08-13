#[derive(Debug, Clone, PartialEq, serde::Serialize, serde::Deserialize)]
pub struct Path {
    pub path: String,
}

#[derive(Debug, Clone, PartialEq, serde::Serialize, serde::Deserialize)]
pub struct Trimmed {
    pub path: String,
    pub out: String,
}
