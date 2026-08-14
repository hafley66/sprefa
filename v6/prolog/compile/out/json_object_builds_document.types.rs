#[derive(Debug, Clone, PartialEq, serde::Serialize, serde::Deserialize)]
pub struct RepoKv {
    pub repo: String,
    pub key: String,
    pub value: serde_json::Value,
}

#[derive(Debug, Clone, PartialEq, serde::Serialize, serde::Deserialize)]
pub struct RepoMeta {
    pub repo: String,
    pub col2: serde_json::Value,
}
