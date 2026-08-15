#[derive(Debug, Clone, PartialEq, serde::Serialize, serde::Deserialize)]
pub struct BoxList {
    pub tree_id: i64,
    pub items: Vec<String>,
}

#[derive(Debug, Clone, PartialEq, serde::Serialize, serde::Deserialize)]
pub struct Patch {
    pub label: String,
    pub at: Plot,
}

#[derive(Debug, Clone, PartialEq, serde::Serialize, serde::Deserialize)]
pub struct Plot {
    pub row: i64,
    pub col: i64,
}

#[derive(Debug, Clone, PartialEq, serde::Serialize, serde::Deserialize)]
pub struct Tree {
    pub tree_id: i64,
    pub species: String,
    pub site: Patch,
}

#[derive(Debug, Clone, PartialEq, serde::Serialize, serde::Deserialize)]
pub struct TreeLabel {
    pub tree_id: i64,
    pub label: String,
}
