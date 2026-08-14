#[derive(Debug, Clone, PartialEq, serde::Serialize, serde::Deserialize)]
pub struct KnownRepo {
    pub to_repo_id: i64,
}

#[derive(Debug, Clone, PartialEq, serde::Serialize, serde::Deserialize)]
pub struct PinExtracted {
    pub from_span_id: i64,
    pub to_repo_id: i64,
    pub to_rev_id: i64,
    pub to_path: String,
    pub kind: String,
}

#[derive(Debug, Clone, PartialEq, serde::Serialize, serde::Deserialize)]
pub struct RepoCandidate {
    pub to_repo_id: i64,
}

#[derive(Debug, Clone, PartialEq, serde::Serialize, serde::Deserialize)]
pub struct Xref {
    pub from_span_id: i64,
    pub to_repo_id: i64,
    pub to_rev_id: i64,
    pub to_path: String,
    pub col5: String,
    pub kind: String,
}
