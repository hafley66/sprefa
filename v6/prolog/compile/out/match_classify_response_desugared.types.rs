#[derive(Debug, Clone, PartialEq, serde::Serialize, serde::Deserialize)]
pub struct FetchResultError {
    pub endpoint: String,
    pub status: i64,
}

#[derive(Debug, Clone, PartialEq, serde::Serialize, serde::Deserialize)]
pub struct FetchResultFresh {
    pub endpoint: String,
    pub tag: String,
    pub body: String,
}

#[derive(Debug, Clone, PartialEq, serde::Serialize, serde::Deserialize)]
pub struct FetchResultUnchanged {
    pub endpoint: String,
}

#[derive(Debug, Clone, PartialEq, serde::Serialize, serde::Deserialize)]
pub struct RespRaw {
    pub endpoint: String,
    pub status: i64,
    pub tag: String,
    pub body: String,
}
