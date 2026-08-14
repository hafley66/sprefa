#[derive(Debug, Clone, PartialEq, serde::Serialize, serde::Deserialize)]
pub struct PullRequest {
    pub number: i64,
    pub title: String,
    pub author: String,
}

#[derive(Debug, Clone, PartialEq, serde::Serialize, serde::Deserialize)]
pub struct Resp {
    pub body: serde_json::Value,
}
