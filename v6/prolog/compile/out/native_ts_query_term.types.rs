#[derive(Debug, Clone, PartialEq, serde::Serialize, serde::Deserialize)]
pub struct Captured {
    pub capture: String,
}

#[derive(Debug, Clone, PartialEq, serde::Serialize, serde::Deserialize)]
pub struct FileDigest {
    pub file_digest: String,
}

#[derive(Debug, Clone, PartialEq, serde::Serialize, serde::Deserialize)]
pub struct Interval {
    pub period: i64,
    pub bucket: i64,
}

#[derive(Debug, Clone, PartialEq, serde::Serialize, serde::Deserialize)]
pub struct QuerySource {
    pub col1: String,
}

#[derive(Debug, Clone, PartialEq, serde::Serialize, serde::Deserialize)]
pub struct QueryValue {
    pub query: String,
}
