#[derive(Debug, Clone, PartialEq, serde::Serialize, serde::Deserialize)]
pub struct Labelled {
    pub tree_id: i64,
    pub label: String,
}

#[derive(Debug, Clone, PartialEq, serde::Serialize, serde::Deserialize)]
pub struct Name {
    pub tree_id: i64,
    pub label: String,
}

#[derive(Debug, Clone, PartialEq, serde::Serialize, serde::Deserialize)]
pub struct Ping {
    pub tree_id: i64,
}
