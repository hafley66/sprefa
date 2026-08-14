#[derive(Debug, Clone, PartialEq, serde::Serialize, serde::Deserialize)]
pub struct DemandRev {
    pub dep_repo_id: i64,
    pub ref_text: String,
}

#[derive(Debug, Clone, PartialEq, serde::Serialize, serde::Deserialize)]
pub struct PinWant {
    pub col1: i64,
    pub dep_repo_id: i64,
    pub ref_text: String,
}

#[derive(Debug, Clone, PartialEq, serde::Serialize, serde::Deserialize)]
pub struct RevFill {
    pub dep_repo_id: i64,
    pub ref_text: String,
    pub behind: i64,
    pub ahead: i64,
}

#[derive(Debug, Clone, PartialEq, serde::Serialize, serde::Deserialize)]
pub struct RevStatus {
    pub dep_repo_id: i64,
    pub ref_text: String,
    pub behind: i64,
    pub ahead: i64,
}

#[derive(Debug, Clone, PartialEq, serde::Serialize, serde::Deserialize)]
pub struct StalePin {
    pub dep_repo_id: i64,
    pub ref_text: String,
}
