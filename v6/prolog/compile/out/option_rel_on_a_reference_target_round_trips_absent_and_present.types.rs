#[derive(Debug, Clone, PartialEq, serde::Serialize, serde::Deserialize)]
pub struct Audit {
    pub audit_id: i64,
    pub at_commit: Commit,
}

#[derive(Debug, Clone, PartialEq, serde::Serialize, serde::Deserialize)]
pub struct Commit {
    pub id: i64,
}

#[derive(Debug, Clone, PartialEq, serde::Serialize, serde::Deserialize)]
pub struct Person {
    pub id: i64,
    pub name: String,
}

#[derive(Debug, Clone, PartialEq, serde::Serialize, serde::Deserialize)]
pub struct Reviewed {
    pub commit_id: i64,
    pub reviewer_name: String,
}
