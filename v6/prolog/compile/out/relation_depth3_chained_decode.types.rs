#[derive(Debug, Clone, PartialEq, serde::Serialize, serde::Deserialize)]
pub struct Dfound {
    pub path_name: String,
    pub kind: String,
}

#[derive(Debug, Clone, PartialEq, serde::Serialize, serde::Deserialize)]
pub struct File {
    pub repo: Repo,
    pub at: Fpath,
}

#[derive(Debug, Clone, PartialEq, serde::Serialize, serde::Deserialize)]
pub struct Fpath {
    pub name: String,
}

#[derive(Debug, Clone, PartialEq, serde::Serialize, serde::Deserialize)]
pub struct Located {
    pub span: Span,
    pub kind: String,
}

#[derive(Debug, Clone, PartialEq, serde::Serialize, serde::Deserialize)]
pub struct Nfound {
    pub path_name: String,
    pub kind: String,
}

#[derive(Debug, Clone, PartialEq, serde::Serialize, serde::Deserialize)]
pub struct Rawk {
    pub repo_name: String,
    pub path_name: String,
    pub start: i64,
    pub end: i64,
    pub kind: String,
}

#[derive(Debug, Clone, PartialEq, serde::Serialize, serde::Deserialize)]
pub struct Repo {
    pub name: String,
}

#[derive(Debug, Clone, PartialEq, serde::Serialize, serde::Deserialize)]
pub struct Span {
    pub file: File,
    pub start: i64,
    pub end: i64,
}
