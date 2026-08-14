#[derive(Debug, Clone, PartialEq, serde::Serialize, serde::Deserialize)]
pub struct CurrentTree {
    pub path: String,
    pub digest: String,
}

#[derive(Debug, Clone, PartialEq, serde::Serialize, serde::Deserialize)]
pub struct Head {
    pub repo_id: i64,
    pub rev_id: i64,
}

#[derive(Debug, Clone, PartialEq, serde::Serialize, serde::Deserialize)]
pub struct HeadMove {
    pub repo_id: i64,
    pub rev_id: i64,
}

#[derive(Debug, Clone, PartialEq, serde::Serialize, serde::Deserialize)]
pub struct TreeFile {
    pub rev_id: i64,
    pub path: String,
    pub digest: String,
}
