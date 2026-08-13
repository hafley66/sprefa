#[derive(Debug, Clone, PartialEq, serde::Serialize, serde::Deserialize)]
pub struct Counted {
    pub repo: String,
    pub stars: i64,
}

#[derive(Debug, Clone, PartialEq, serde::Serialize, serde::Deserialize)]
pub struct Event {
    pub payload: serde_json::Value,
}
