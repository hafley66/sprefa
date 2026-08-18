#[derive(Debug, Clone, PartialEq, serde::Serialize, serde::Deserialize)]
pub struct Grove {
    pub id: i64,
}

#[derive(Debug, Clone, PartialEq, serde::Serialize, serde::Deserialize)]
pub struct Tree {
    pub id: i64,
    pub name: String,
}

#[derive(Debug, Clone, PartialEq, serde::Serialize, serde::Deserialize)]
pub struct Planted {
    pub grove_id: i64,
    pub tree_name: String,
}

#[derive(Debug, Clone, PartialEq, serde::Serialize, serde::Deserialize)]
pub struct Orchard {
}
