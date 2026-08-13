#[derive(Debug, Clone, PartialEq, serde::Serialize, serde::Deserialize)]
pub struct Repo {
    pub name: String,
    pub tags: Vec<String>,
}

#[derive(Debug, Clone, PartialEq, serde::Serialize, serde::Deserialize)]
pub struct RepoTag {
    pub name: String,
    pub tag: String,
}
