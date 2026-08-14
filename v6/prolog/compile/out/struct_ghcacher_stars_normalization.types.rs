#[derive(Debug, Clone, PartialEq, serde::Serialize, serde::Deserialize)]
pub struct CurrentBody {
    pub ep: String,
    pub body: RepoBody,
}

#[derive(Debug, Clone, PartialEq, serde::Serialize, serde::Deserialize)]
pub struct RepoBody {
    pub full_name: String,
    pub stargazers_count: i64,
}

#[derive(Debug, Clone, PartialEq, serde::Serialize, serde::Deserialize)]
pub struct Stars {
    pub ep: String,
    pub n: i64,
}
