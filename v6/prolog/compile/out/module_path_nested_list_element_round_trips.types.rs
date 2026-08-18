#[derive(Debug, Clone, PartialEq, serde::Serialize, serde::Deserialize)]
pub struct Grove {
    pub id: i64,
    pub members: Vec<Vec<Tree>>,
}

#[derive(Debug, Clone, PartialEq, serde::Serialize, serde::Deserialize)]
pub struct Tree {
    pub name: String,
    pub url: String,
}

#[derive(Debug, Clone, PartialEq, serde::Serialize, serde::Deserialize)]
pub struct Orchard {
}
