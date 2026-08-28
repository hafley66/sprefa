#[derive(Debug, Clone, PartialEq, serde::Serialize, serde::Deserialize)]
pub struct AnyTree {
    pub tree_id: i64,
}

#[derive(Debug, Clone, PartialEq, serde::Serialize, serde::Deserialize)]
pub struct Orchard {
    pub orchard_id: i64,
}

#[derive(Debug, Clone, PartialEq, serde::Serialize, serde::Deserialize)]
pub struct Tree {
    pub tree_id: i64,
}

#[derive(Debug, Clone, PartialEq, serde::Serialize, serde::Deserialize)]
pub struct Planted {
    pub orchard_id: i64,
    pub tree_id: i64,
}
