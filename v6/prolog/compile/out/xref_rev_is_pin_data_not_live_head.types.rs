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
pub struct KnownRepo {
    pub col1: i64,
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
pub struct Xref {
    pub from_span_id: i64,
    pub to_repo_id: i64,
    pub to_rev_id: i64,
    pub to_path: String,
    pub col5: String,
    pub kind: String,
}
