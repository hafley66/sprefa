#[derive(Debug, Clone, PartialEq, serde::Serialize, serde::Deserialize)]
pub struct Labelled {
    pub tree_id: i64,
    pub state: String,
}

#[derive(Debug, Clone, PartialEq, serde::Serialize, serde::Deserialize)]
pub struct Tree {
    pub tree_id: i64,
    pub label: i64,
}

#[derive(Debug, Clone, PartialEq, serde::Serialize, serde::Deserialize)]
pub struct Orchard {
}
