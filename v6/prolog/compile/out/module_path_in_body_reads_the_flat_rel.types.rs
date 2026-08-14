#[derive(Debug, Clone, PartialEq, serde::Serialize, serde::Deserialize)]
pub struct Fruit {
    pub tree_id: i64,
    pub picked: i64,
}

#[derive(Debug, Clone, PartialEq, serde::Serialize, serde::Deserialize)]
pub struct Ripe {
    pub tree_id: i64,
}

#[derive(Debug, Clone, PartialEq, serde::Serialize, serde::Deserialize)]
pub struct Orchard {
}
