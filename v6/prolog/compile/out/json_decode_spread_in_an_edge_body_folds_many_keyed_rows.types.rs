#[derive(Debug, Clone, PartialEq, serde::Serialize, serde::Deserialize)]
pub struct PullPage {
    pub doc: serde_json::Value,
}

#[derive(Debug, Clone, PartialEq, serde::Serialize, serde::Deserialize)]
pub struct PullState {
    pub number: i64,
    pub title: String,
}
