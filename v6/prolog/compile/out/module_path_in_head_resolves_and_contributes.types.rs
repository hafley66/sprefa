#[derive(Debug, Clone, PartialEq, serde::Serialize, serde::Deserialize)]
pub struct Harvest {
    pub tree_id: i64,
    pub picked: i64,
}

#[derive(Debug, Clone, PartialEq, serde::Serialize, serde::Deserialize)]
pub struct Tree {
    pub tree_id: i64,
    pub picked: i64,
}

#[derive(Debug, Clone, PartialEq, serde::Serialize, serde::Deserialize)]
pub struct Orchard {
}
