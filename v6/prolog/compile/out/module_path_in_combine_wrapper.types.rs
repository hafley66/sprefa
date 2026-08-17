#[derive(Debug, Clone, PartialEq, serde::Serialize, serde::Deserialize)]
pub struct Plot {
    pub tree_id: i64,
    pub plot: String,
}

#[derive(Debug, Clone, PartialEq, serde::Serialize, serde::Deserialize)]
pub struct Tree {
    pub tree_id: i64,
}

#[derive(Debug, Clone, PartialEq, serde::Serialize, serde::Deserialize)]
pub struct Sited {
    pub tree_id: i64,
    pub plot: String,
}

#[derive(Debug, Clone, PartialEq, serde::Serialize, serde::Deserialize)]
pub struct Orchard {
}
