#[derive(Debug, Clone, PartialEq, serde::Serialize, serde::Deserialize)]
pub struct Grew {
    pub orchard_id: i64,
    pub tree_id: i64,
    pub branch_id: i64,
}

#[derive(Debug, Clone, PartialEq, serde::Serialize, serde::Deserialize)]
pub struct Orchard {
    pub orchard_id: i64,
}

#[derive(Debug, Clone, PartialEq, serde::Serialize, serde::Deserialize)]
pub struct Tree {
    pub parent: Orchard,
    pub tree_id: i64,
}

#[derive(Debug, Clone, PartialEq, serde::Serialize, serde::Deserialize)]
pub struct Branch {
    pub parent: Tree,
    pub branch_id: i64,
}
