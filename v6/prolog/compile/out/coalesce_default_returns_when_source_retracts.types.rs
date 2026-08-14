#[derive(Debug, Clone, PartialEq, serde::Serialize, serde::Deserialize)]
pub struct LatestCommit {
    pub name: String,
    pub commit: String,
}

#[derive(Debug, Clone, PartialEq, serde::Serialize, serde::Deserialize)]
pub struct Repo {
    pub name: String,
}

#[derive(Debug, Clone, PartialEq, serde::Serialize, serde::Deserialize)]
pub struct RepoLatest {
    pub name: String,
    pub commit: String,
}
